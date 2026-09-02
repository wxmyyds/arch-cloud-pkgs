#!/usr/bin/env bash
# =====================================================================
# 构建工具链准备脚本 — 由 .github/workflows/build.yml 的 build job 调用
# 运行环境: archlinux:base-devel 容器, root 用户
# 用法:     prepare-toolchain.sh <包目录>
#
# 读取 <包目录>/build.conf 的声明式配置（shell 语法 key=value）:
#   makepkg_flags      传给 makepkg 的额外参数（如交叉编译需要 --ignorearch）
#   extra_pacman_deps  空格分隔的额外 pacman 依赖
#   needs_ndk          是否需要 Android NDK (true/false)
#   ndk_version        NDK 版本号，默认 r26d
#   ndk_sha1           NDK Linux zip 的 SHA-1（needs_ndk=true 时必填）
#   ndk_platform       Android API level，默认 28
#   rust_targets       空格分隔的 rustup 交叉 target
#   needs_cargo_ndk    是否需要 cargo-ndk (true/false)
#   cache_dirs         编译缓存目录（相对包目录），默认 target
#   extra_artifacts    额外上传产物（构建后由工作流收集）
#
# build.conf 在工作流中经过白名单校验后解析，结果经 GITHUB_ENV 注入本脚本环境；
# 本脚本自行解析受控赋值，既支持 CI 也支持脱离 CI 单独运行。
# 无 build.conf 的包仅跳过"按包工具链准备"，通用准备仍执行
# （makepkg -s 会按 PKGBUILD makedepends 自动安装）
# =====================================================================
set -euo pipefail

d="${1:?用法: prepare-toolchain.sh <包目录>}"
conf="$d/build.conf"

# --- 0. 通用准备（所有包）：固定 cargo/rustup 家目录 + 源码下载缓存 ---
# CARGO_HOME / RUSTUP_HOME 固定成共享路径，让 root（本脚本）与 builder（makepkg）
# 共用同一份 registry 与工具链。不这么做的话 rustup 会按 $HOME 分裂成
# /root/.rustup 与 /home/builder/.rustup 两份，target 得装两次。
# 注意：makepkg 是 `sudo -u builder` 跑的，sudo 会重置环境变量，所以这两个值
# 在 PKGBUILD 里用同样的默认值又兜底了一遍，改动时两边要同步。
export CARGO_HOME="${CARGO_HOME:-/home/builder/.cargo}"
export RUSTUP_HOME="${RUSTUP_HOME:-/opt/rustup}"

# SRCDEST：tarball/git 的只读内容寻址副本，共享无害（工具链层资源）；
# 与包目录 src/（解压后的构建现场）分离——缓存只保存下载物，不保存构建现场。
# cargo registry/git 同理归工具链层（内容寻址只读），但缓存以 root 恢复后
# 必须归还 builder（makepkg 以 builder 运行，--frozen/--locked 也要写 registry）
mkdir -p /opt/cache/sources "$CARGO_HOME" "$RUSTUP_HOME"
chown -R builder:builder /opt/cache/sources "$CARGO_HOME" "$RUSTUP_HOME"
echo "SRCDEST=/opt/cache/sources" >> /etc/makepkg.conf

if [ ! -f "$conf" ]; then
  echo "::notice::$d 无 build.conf，跳过按包工具链准备"
  exit 0
fi

echo "::group::准备工具链 $d"
cat "$conf"
# 逐项解析受控变量赋值，不把仓库配置当作任意 shell 脚本执行
while IFS= read -r line || [ -n "$line" ]; do
  trimmed="${line#"${line%%[![:space:]]*}"}"
  [ -z "$trimmed" ] && continue
  case "$trimmed" in
    \#*) continue ;;
  esac
  if [[ ! "$trimmed" =~ ^([A-Za-z_][A-Za-z0-9_]*)=(.*)$ ]]; then
    echo "::error::build.conf 只能包含变量赋值: $line"
    exit 1
  fi
  key="${BASH_REMATCH[1]}"
  raw="${BASH_REMATCH[2]}"
  case "$key" in
    needs_ndk|ndk_version|ndk_sha1|ndk_platform|cache_dirs|extra_artifacts|makepkg_flags|rust_targets|needs_cargo_ndk|extra_pacman_deps) ;;
    *)
      echo "::error::build.conf 包含不支持的字段: $key"
      exit 1
      ;;
  esac
  case "$raw" in
    \"*\") value="${raw:1:${#raw}-2}" ;;
    \'*\') value="${raw:1:${#raw}-2}" ;;
    *) value="$raw" ;;
  esac
  if [[ ! "$raw" =~ ^[A-Za-z0-9_./:+,\ -]*$ && ! "$raw" =~ ^\"[A-Za-z0-9_./:+,\ -]*\"$ && ! "$raw" =~ ^\'[A-Za-z0-9_./:+,\ -]*\'$ ]]; then
    echo "::error::build.conf 含有不安全字符: $line"
    exit 1
  fi
  printf -v "$key" '%s' "$value"
done < "$conf"

# --- 1. 额外 pacman 依赖（构建前提，失败即失败，尽早暴露）---
if [ -n "${extra_pacman_deps:-}" ]; then
  echo "安装 extra_pacman_deps: $extra_pacman_deps"
  # shellcheck disable=SC2086
  pacman -S --needed --noconfirm $extra_pacman_deps
fi

# --- 2. Android NDK（官方仓库无此包，Google 官方 zip 直装；缓存命中则跳过）---
if [ "${needs_ndk:-false}" = "true" ]; then
  VER="${ndk_version:-r26d}"
  NDK_SHA1="${ndk_sha1:-}"
  if [ -x /opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/clang ]; then
    echo "::notice::/opt/android-ndk 已就绪（缓存命中），跳过下载"
  else
    echo "下载 android-ndk $VER（约 1.2GB）..."
    tmp_ndk="$(mktemp /tmp/android-ndk.XXXXXX.zip)"
    trap 'rm -f "$tmp_ndk"' EXIT
    curl --fail --show-error --location --retry 3 --retry-delay 5 --retry-all-errors \
      "https://dl.google.com/android/repository/android-ndk-${VER}-linux.zip" -o "$tmp_ndk"
    if [ -z "$NDK_SHA1" ]; then
      echo "::error::未配置 ndk_sha1，拒绝使用未经校验的 NDK 下载"
      exit 1
    fi
    printf '%s  %s\n' "$NDK_SHA1" "$tmp_ndk" | sha1sum -c -
    ndk_tmp_dir="$(mktemp -d /opt/android-ndk.XXXXXX)"
    unzip -q "$tmp_ndk" -d "$ndk_tmp_dir"
    mv "$ndk_tmp_dir/android-ndk-${VER}" /opt/android-ndk
    rmdir "$ndk_tmp_dir"
    rm -f "$tmp_ndk"
    trap - EXIT
  fi
  # 缓存/直装均以 root 落地，归还给 builder（makepkg 用户）
  chown -R builder:builder /opt/android-ndk
  # NDK 的 clang 包装器依赖同目录 clang，必须整目录进 PATH（ln -sf 单文件不够）
  if [ -n "${GITHUB_ENV:-}" ]; then
    echo "ANDROID_NDK_HOME=/opt/android-ndk" >> "$GITHUB_ENV"
    echo "NDK_PLATFORM=${ndk_platform:-28}" >> "$GITHUB_ENV"
  fi
  if [ -n "${GITHUB_PATH:-}" ]; then
    echo "/opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin" >> "$GITHUB_PATH"
  else
    export PATH="/opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin:$PATH"
  fi
fi

# --- 3. rustup + 交叉 target ---
# 只在需要交叉编译时接管 Rust 工具链；原生包继续用 Arch 官方的 rust。
# rustup 包 conflicts=('cargo' 'rust' 'rustfmt')，两者互斥，不能混用。
if [ -n "${rust_targets:-}" ]; then
  pacman -S --needed --noconfirm rustup
  # CARGO_HOME/RUSTUP_HOME 已固定为共享路径，root 和 builder 共用一份工具链，
  # 只需装一次（旧版按 root/builder 各装一遍，白花一倍时间还容易装出两份）
  rustup toolchain install stable --profile minimal
  rustup default stable
  for t in $rust_targets; do
    echo "添加 rust target: $t"
    rustup target add "$t"
  done
  # 让 $CARGO_HOME/bin（cargo install 的产物，如 cargo-ndk）对所有人可见
  if [ -n "${GITHUB_PATH:-}" ]; then
    echo "$CARGO_HOME/bin" >> "$GITHUB_PATH"
  else
    export PATH="$CARGO_HOME/bin:$PATH"
  fi
  chown -R builder:builder "$RUSTUP_HOME" "$CARGO_HOME"
fi

# --- 4. cargo-ndk（Android 包的必需构建工具）---
if [ "${needs_cargo_ndk:-false}" = "true" ]; then
  # 缓存恢复的 registry 以 root 落地，先归还 builder 再安装（builder 的 cargo 才被 makepkg 用到）
  chown -R builder:builder "$CARGO_HOME"
  if ! command -v cargo-ndk >/dev/null 2>&1 && [ ! -x /usr/local/bin/cargo-ndk ]; then
    echo "安装 cargo-ndk"
    cargo install cargo-ndk --locked
    # CARGO_HOME 是共享的，装完就落在 $CARGO_HOME/bin（已进 PATH）。
    # 再复制一份到 /usr/local/bin 兜底：sudo -u builder 会重置环境变量，
    # PKGBUILD 里未必看得到 $CARGO_HOME/bin，而 /usr/local/bin 默认在 PATH。
    # 找不到 cargo-ndk 时 PKGBUILD 会 fallback 纯 cargo 交叉 →
    # 用 cc 链接 → 找不到 NDK 的 -llog/-lunwind
    if [ -x "$CARGO_HOME/bin/cargo-ndk" ]; then
      cp -f "$CARGO_HOME/bin/cargo-ndk" /usr/local/bin/cargo-ndk
    fi
  fi
  # cargo-ndk 是 Cargo 子命令，直接执行 cargo-ndk 会主动报错：
  # "This binary may only be called via cargo ndk"。
  cargo ndk --version
fi

echo "::endgroup::"
