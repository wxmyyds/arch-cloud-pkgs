#!/usr/bin/env bash
# =====================================================================
# 构建工具链准备脚本 — 由 .github/workflows/build.yml 的 build job 调用
# 运行环境: archlinux:base-devel 容器, root 用户
# 用法:     prepare-toolchain.sh <包目录>
#
# 读取 <包目录>/build.conf 的声明式配置（shell 语法 key=value）:
#   extra_pacman_deps  空格分隔的额外 pacman 依赖
#   needs_ndk          是否需要 Android NDK (true/false)
#   ndk_version        NDK 版本号，默认 r26d
#   rust_targets       空格分隔的 rustup 交叉 target
#   needs_cargo_ndk    是否需要 cargo-ndk (true/false)
#   extra_artifacts    额外上传产物（构建后由工作流收集）
#
# 无 build.conf 的包直接跳过（makepkg -s 会按 makedepends 自动装）。
# =====================================================================
set -euo pipefail

d="${1:?用法: prepare-toolchain.sh <包目录>}"
conf="$d/build.conf"

if [ ! -f "$conf" ]; then
  echo "::notice::$d 无 build.conf，跳过工具链准备"
  exit 0
fi

echo "::group::准备工具链 $d"
cat "$conf"
# 加载声明（shell 语法，天然支持注释）
set -a; source "$conf"; set +a

# 向后续步骤暴露声明值（供缓存 key、上传路径等使用）
echo "needs_ndk=${needs_ndk:-false}" >> "$GITHUB_OUTPUT"
echo "ndk_version=${ndk_version:-r26d}" >> "$GITHUB_OUTPUT"
echo "extra_artifacts=${extra_artifacts:-}" >> "$GITHUB_OUTPUT"

# --- 1. 额外 pacman 依赖（构建前提，失败即失败，尽早暴露）---
if [ -n "${extra_pacman_deps:-}" ]; then
  echo "安装 extra_pacman_deps: $extra_pacman_deps"
  # shellcheck disable=SC2086
  pacman -S --needed --noconfirm $extra_pacman_deps
fi

# --- 2. Android NDK（官方仓库无此包，Google 官方 zip 直装；缓存命中则跳过）---
if [ "${needs_ndk:-false}" = "true" ]; then
  VER="${ndk_version:-r26d}"
  if [ -x /opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin/clang ]; then
    echo "::notice::/opt/android-ndk 已就绪（缓存命中），跳过下载"
  else
    echo "下载 android-ndk $VER（约 1.2GB）..."
    curl -fsSL "https://dl.google.com/android/repository/android-ndk-${VER}-linux.zip" -o /tmp/ndk.zip
    mkdir -p /opt
    unzip -q /tmp/ndk.zip -d /opt
    mv "/opt/android-ndk-${VER}" /opt/android-ndk
    rm -f /tmp/ndk.zip
  fi
  # 缓存/直装均以 root 落地，归还给 builder（makepkg 用户）
  chown -R builder:builder /opt/android-ndk 2>/dev/null || true
  # NDK 的 clang 包装器依赖同目录 clang，必须整目录进 PATH（ln -sf 单文件不够）
  echo "ANDROID_NDK_HOME=/opt/android-ndk" >> "$GITHUB_ENV"
  echo "/opt/android-ndk/toolchains/llvm/prebuilt/linux-x86_64/bin" >> "$GITHUB_PATH"
fi

# --- 3. rustup + 交叉 target（cargo-ndk 的备胎路径；网络操作，容错）---
if [ -n "${rust_targets:-}" ]; then
  pacman -S --needed --noconfirm rustup 2>&1 || true
  # root 与 builder 双份装（makepkg 以 builder 运行，其 rustup 配置独立）
  rustup toolchain install stable --profile minimal 2>&1 || true
  rustup default stable 2>&1 || true
  sudo -u builder bash -c 'rustup toolchain install stable --profile minimal 2>&1 || true; rustup default stable 2>&1 || true' || true
  for t in $rust_targets; do
    echo "添加 rust target: $t"
    rustup target add "$t" 2>&1 || true
    sudo -u builder rustup target add "$t" 2>&1 || true
  done
fi

# --- 4. cargo-ndk（Android 包的主构建路径；装不上时 PKGBUILD 内 fallback 纯 cargo）---
if [ "${needs_cargo_ndk:-false}" = "true" ]; then
  # 缓存恢复的 registry 以 root 落地，先归还 builder 再安装（builder 的 cargo 才被 makepkg 用到）
  chown -R builder:builder /home/builder/.cargo 2>/dev/null || true
  if ! command -v cargo-ndk >/dev/null 2>&1 && [ ! -x /usr/local/bin/cargo-ndk ]; then
    echo "安装 cargo-ndk"
    cargo install cargo-ndk --locked 2>&1 || true
    # makepkg 以 builder 运行，root 的 ~/.cargo/bin 对它不可见；
    # 复制到 /usr/local/bin（默认在 PATH）让所有用户可用，否则 PKGBUILD 会
    # fallback 到纯 cargo 交叉编译 → 用 cc 链接 → 找不到 NDK 的 -llog/-lunwind
    if command -v cargo-ndk >/dev/null 2>&1; then
      cp -f "$(command -v cargo-ndk)" /usr/local/bin/cargo-ndk
    fi
    # root 装失败则给 builder 装（其 rustup/cargo 独立）
    if [ ! -x /usr/local/bin/cargo-ndk ]; then
      sudo -u builder bash -c 'cargo install cargo-ndk --locked' 2>&1 || true
      [ -x /home/builder/.cargo/bin/cargo-ndk ] && cp -f /home/builder/.cargo/bin/cargo-ndk /usr/local/bin/cargo-ndk 2>/dev/null || true
    fi
  fi
  /usr/local/bin/cargo-ndk --version 2>&1 || cargo-ndk --version 2>&1 || true
fi

echo "::endgroup::"
