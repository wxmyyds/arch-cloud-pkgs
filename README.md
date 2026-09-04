# arch-cloud-pkgs

个人 Arch Linux 包云端构建仓库：用 GitHub Actions 在 `archlinux` 容器里跑 `makepkg`，本机零编译零依赖污染。

## 已收录包

| 包名 | 上游 | 说明 |
|------|------|------|
| [we-layerd](https://github.com/Aromatic05/we-layerd) | v0.2.7 | 原生 Wallpaper Engine 运行时（scene/video/web，支持 niri）|
| [wayland-pipewire-idle-inhibit-aur](https://github.com/rafaelrc7/wayland-pipewire-idle-inhibit) | v0.7.1 | 播放声音时抑制 Wayland idle（包名带 `-aur` 后缀以避免产物匹配问题，`provides` 原包名）|
| [rtk-termux](https://github.com/rtk-ai/rtk) | v0.47.0 | 交叉编译的 Termux aarch64 版（上游只发 gnu/musl 预编译，没有 Bionic）。**不要在本机 Arch 上安装**，见下 |
| [zcode](https://zcode.z.ai) | 官网 latest（构建时解析） | Z.ai 官方 Electron 桌面应用重打包（AppImage → 原生包）。版本自动跟最新：上游发版**无需改文件**，定时重建或手动触发即取当时最新。与 AUR `z-code-bin` 互为冲突，安装时 pacman 会提示替换 |

## 使用方法

### 触发构建
1. 推送代码后自动触发；或到 **Actions → Build packages → Run workflow** 手动触发
2. 另外每月 1 号会定时全量重建一次（Arch 滚动更新，glibc/gcc-libs 的 soname bump 会让旧包静默失效）
3. 等待构建完成（见下方"构建加速"）
4. 到该次运行页面底部下载对应包的 artifact `arch-packages-<包名>`；每个包单独一个文件，互不影响

> 产物保留 30 天（`retention-days: 30`），过期即不可下载。需要长期留存的请配合三期 Roadmap 的私人仓库。

### 本地安装
```bash
sudo pacman -U *.pacman
```

**两个前置坑：**

- `we-layerd` 依赖 `cef` 和 `directx-shader-compiler`，这两个都在 AUR、不在官方仓库里。
  直接用 `pacman -U` 会因缺依赖报错，需先用 AUR helper 装好：
  ```bash
  paru -S cef directx-shader-compiler
  sudo pacman -U we-layerd-*.pacman
  ```
- `rtk-termux` 的 `arch=('aarch64')`，产物是 Android Bionic 二进制，**在 x86_64 主机上会被 pacman 的架构检查拦下**——这是刻意的保护，装进去会顶掉正常的 `/usr/bin/rtk`。
  给 Termux 用的话取 artifact 里的裸二进制 `rtk-aarch64-android` 即可，不需要走 pacman。

### 升级某个包
编辑对应 `pkgs/<包名>/PKGBUILD` 的 `pkgver`，commit & push 即可自动重新构建。

- 源是 GitHub tarball 的包（`rtk-termux`、`wayland-pipewire-idle-inhibit-aur`）
  必须同时更新 `sha256sums`：
  ```bash
  curl -sL <tarball url> | sha256sum
  ```
- 源是 git tag 的包（`we-layerd`）保持 `SKIP`。
- `zcode` 不需要"升级"：`pkgver` 在构建时从官网安装文档页解析最新版（解析失败会
  直接报错终止，不会打出错误版本）。它是唯一用 `SKIP` 的 tarball 源——浮动版本
  无法预钉哈希，完整性由 `prepare()` 的体积下限 + AppImage 魔数校验兜底。

改了 `pkgver` 就属于改了 PKGBUILD，缓存 key 随之变化；但由于配了 `restore-keys`，
会命中上一次的 `target/` 做**增量**重编（cargo 只重编变化的 crate），不是全量重编。

## 构建加速

- **只构建变更的包**：push 时按 diff 选出改动涉及的包，也可手动指定单个包
- **并行构建**：多个包同时变更时，每个包一个独立 job（matrix）并行执行，互不影响；产物也按包分别上传
- **缓存分层**：
  - 工具链层（跨包共享）：NDK、cargo registry/git、`SRCDEST` 源码下载副本——都是内容寻址的只读资源
  - 包层（按包隔离）：`cache_dirs` 声明的编译目录（Rust 约定 `target/`），key 含包名 + PKGBUILD + build.conf + NDK 版本 + 架构
  - `src/`（解压后的源码现场）**不缓存**，每次由 makepkg 从 `SRCDEST` 重新获取，避免"源码与中间产物混杂"的脏缓存
  - 缓存受 GitHub 10GB/仓库 配额限制，旧版本缓存会自动淘汰
- **自动取消**：同一分支的新提交会取消仍在跑的旧构建，避免浪费 runner

## 添加新包

```bash
mkdir pkgs/<新包名>
# 参照 pkgs/we-layerd/PKGBUILD 编写
git add && git commit && git push   # 自动触发构建
```

`build.conf` 是可选的声明式配置（shell 语法 `key=value`），字段说明见
[`prepare-toolchain.sh`](.github/scripts/prepare-toolchain.sh) 头部注释。常用字段：

| 字段 | 默认 | 用途 |
|------|------|------|
| `cache_dirs` | `target` | 编译缓存目录（相对包目录） |
| `makepkg_flags` | 空 | 传给 makepkg 的额外参数，如交叉编译包需要 `--ignorearch` |
| `needs_ndk` / `ndk_version` | `false` / `r26d` | 是否下载 Android NDK |
| `ndk_sha1` / `ndk_platform` | 空 / `28` | NDK zip 完整性校验与 Android API level；启用 NDK 时 SHA-1 必填 |
| `rust_targets` | 空 | 需要 `rustup target add` 的交叉 target |
| `needs_cargo_ndk` | `false` | 是否 `cargo install cargo-ndk` |
| `extra_pacman_deps` | 空 | makepkg 之前就要装好的 pacman 依赖 |
| `extra_artifacts` | 空 | 除了 .pacman 之外一并上传的产物 |

## 注意事项

- 构建在 `archlinux:base-devel` 官方容器中进行，glibc 与本机滚动版本一致
- makepkg 以普通用户 `builder`（带 NOPASSWD sudo）运行，可自动解析 makedepends
- `sudo -u builder` 会重置环境变量，workflow 里写进 `GITHUB_ENV` 的值进不了 makepkg。
  所以 NDK 路径、`CARGO_HOME`、`RUSTUP_HOME` 在 PKGBUILD 里都用同样的默认值又兜底了一遍
  ——**改路径时 `prepare-toolchain.sh` 和对应 PKGBUILD 要同步改**
- 交叉编译包只声明 `rustup`，不要和 `rust` 同时写进 `makedepends`：
  Arch 的 rustup 包 `conflicts=('cargo' 'rust' 'rustfmt')`，两者互斥
- 工作流里所有 GitHub 上下文都经 `env:` 传入，不在 `run:` 里直接写 `${{ }}`——
  后者是 shell 解析前的纯文本替换，`workflow_dispatch` 输入里带个分号就能执行任意命令
- `build.conf` 只允许受控的变量赋值，工作流会校验包名、缓存路径、额外产物路径和 NDK 校验值；不要在配置中写 shell 表达式
- 构建完成后会强制检查至少生成一个包，并执行 `pacman -Qip` 与 `namcap`；检查失败会阻止上传产物

## Roadmap

- [x] 一期：增量构建（只构建变更的包）+ 并行 matrix + cargo 缓存
- [ ] 二期：nvchecker 自动检测上游发版并触发构建
- [ ] 三期：Releases 当私人 pacman 仓库（repo-add），实现 pacman -Syu 直接更新
