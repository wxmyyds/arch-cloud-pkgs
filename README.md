# arch-cloud-pkgs

个人 Arch Linux 包云端构建仓库：用 GitHub Actions 在 `archlinux` 容器里跑 `makepkg`，本机零编译零依赖污染。

## 已收录包

| 包名 | 上游 | 说明 |
|------|------|------|
| we-layerd | [Aromatic05/we-layerd](https://github.com/Aromatic05/we-layerd) | 原生 Wallpaper Engine 运行时（scene/video/web，支持 niri）。重打包上游官方预编译 deb，内置私有 CEF/DXC 运行时，版本构建时跟随最新 release。**不再需要 AUR 依赖** |
| wayland-pipewire-idle-inhibit-aur | [rafaelrc7/wayland-pipewire-idle-inhibit](https://github.com/rafaelrc7/wayland-pipewire-idle-inhibit) | 播放声音时抑制 Wayland idle（包名带 `-aur` 后缀以避免产物匹配问题，`provides` 原包名），版本构建时跟随最新 tag，源码编译 |
| rtk-termux | [rtk-ai/rtk](https://github.com/rtk-ai/rtk) | 交叉编译的 Termux aarch64 版（上游只发 gnu/musl 预编译，没有 Bionic），版本构建时跟随最新 release。**不要在本机 Arch 上安装**，见下 |
| zcode | [Z.ai](https://zcode.z.ai) | Z.ai 官方 Electron 桌面应用重打包（AppImage → 原生包）。版本自动跟最新：上游发版**无需改文件**，定时重建或手动触发即取当时最新。与 AUR `z-code-bin` 互为冲突，安装时 pacman 会提示替换 |
| wechat | [腾讯微信](https://linux.weixin.qq.com/) | 腾讯官方微信 Linux x86_64 AppImage 重打包为原生包；仅支持 x86_64，版本自动跟随官网，ARM 版暂未接入 |
| noctalia-greeter | [noctalia-dev/noctalia-greeter](https://github.com/noctalia-dev/noctalia-greeter) | Noctalia 官方 greetd 登录界面（C++20 + wlroots 合成器），源码编译，版本构建时自动跟随上游最新 tag。替代旧包 `dank-greeter`（DMS 登录界面）；装完编辑 `/etc/greetd/config.toml` 指向 `noctalia-greeter-session` |
| mark-shot | [jswysnemc/mark-shot](https://github.com/jswysnemc/mark-shot) | Qt6 Wayland 截图标注工具，重打包上游官方预编译 Arch 包（依赖、layer-shell 库与翻译插件齐全，二进制字节保真），版本构建时跟随最新 release。conflicts AUR `mark-shot-bin`；对本机已装 AUR `mark-shot` 为同名升级 |
| google-chrome | [Google](https://www.google.com/chrome) | Google 官方 Chrome .deb 重打包；版本与 SHA256 从官方 apt 索引动态解析并由 makepkg 落地校验，打包逻辑对齐 AUR 同名包。`paru -Syu` 提示同版本"升级"时跳过 |
| rtk | [rtk-ai/rtk](https://github.com/rtk-ai/rtk) | LLM token 节省代理，Rust musl 静态单文件零依赖。行为仿照官方 install.sh（302 解析 tag + checksums.txt 动态校验 + CWE-22 防御），装到 `/usr/bin/rtk`；装完删除手动装的 `~/.local/bin/rtk` |
| axolotl-launcher | [Mystic-Stars/Axolotl](https://github.com/Mystic-Stars/Axolotl) | 开源跨平台 Minecraft 启动器（Rust + Tauri，Modrinth 生态），重打包上游官方预编译 deb（amd64/arm64 双架构），版本构建时跟随最新 release。conflicts AUR `axolotl-launcher-bin`；对本机已装 AUR `axolotl-launcher`（源码版）为同名升级 |

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

微信包仅构建 x86_64 版本，安装产物为官方 AppImage 解包后的原生 pacman 包，
不需要 FUSE。若已安装其他会占用同名微信文件的原生 Linux 包，请按 pacman 提示先处理冲突。

**前置坑：**

- `we-layerd` 改用上游官方预编译 deb 重打包后，CEF 与 DXC 运行时已内置
  （`/usr/lib/cef/`、`/usr/lib/we-layerd/dxc/`，经 `$ORIGIN` RUNPATH 加载），
  全部依赖都在官方仓库，`pacman -U` 直接装。若之前为旧源码构建版装过 AUR 的
  `cef` / `directx-shader-compiler` 且无其他包依赖它们，可顺手移除。
- `rtk-termux` 的 `arch=('aarch64')`，产物是 Android Bionic 二进制，**在 x86_64 主机上会被 pacman 的架构检查拦下**——这是刻意的保护，装进去会顶掉正常的 `/usr/bin/rtk`。
  给 Termux 用的话取 artifact 里的裸二进制 `rtk-aarch64-android` 即可，不需要走 pacman。
### 升级某个包
所有包的 `pkgver` 均在构建时自动解析上游最新版（release tag / apt 索引 / 官网页面），
**无需改任何文件**：上游发版后定时重建（每月 1 号）或手动 workflow_dispatch 指定包名
即取当时最新版。解析失败或版本号格式异常会直接报错终止构建，不会打出错误版本。

校验值的来源按包而异，均由 makepkg 下载后落地校验：

- `google-chrome`：版本与 SHA256 从官方 apt `Packages` 索引解析。
- `rtk`：deb/tarball 的 SHA256 从该 tag 的 `checksums.txt` 解析。
- `we-layerd`：deb 文件名与 SHA256 从该 tag 的 `SHA256SUMS` 解析。
- `mark-shot`：x86_64 pkg.tar.zst 资产名与 SHA256 从该 tag 的 `*.pkg.tar.zst.sha256`
  解析；上游若改了 pkgrel 导致常规命名探测落空，回退 release 资产页解析实际文件名。
- `noctalia-greeter`：上游无 release 只有 tag，走 `git ls-remote`（不耗 API 配额）
  + tags API 兜底，过滤语义化 tag 后 `sort -V` 取最新；tarball SHA256 同次构建内
  流式计算，`prepare()` 再用 `meson.build` 的 project version 与 `pkgver` 对账。
- `wayland-pipewire-idle-inhibit-aur`：上游无 release 只有 tag，走 `git ls-remote`（不耗
  API 配额）+ tags API 兜底，过滤语义化 tag 后 `sort -V` 取最新；tarball SHA256 同次
  构建内流式计算，`prepare()` 再用 `Cargo.toml` 的 version 与 `pkgver` 对账。
- `rtk-termux`：GitHub archive 源码 tarball 动态生成、官方不承诺内容寻址，
  `sha256sums` 为 `SKIP`；完整性由 `prepare()` 的体积下限 + `Cargo.toml`/`src/`
  结构检查（version 与 `pkgver` 对账）兜底。
- `zcode`/`wechat`：官网未提供固定 SHA-256，`sha256sums` 为 `SKIP`；完整性由
  `prepare()` 的体积下限、AppImage type-2 魔数和解包后的关键文件检查兜底。
  源文件名包含版本号，避免固定下载 URL 在 CI 的 `SRCDEST` 缓存中复用旧版本。
- `axolotl-launcher`：deb 的 SHA256 从该 tag 的 release API `assets[].digest`
  动态解析（上游无 SHA256SUMS 聚合文件、deb 无附带 `.sha256`，API digest 是唯一
  独立校验源，实测与下载文件逐字节一致）；`COPYING.md` 经 raw 流式计算；
  仓库内静态文件（desktop、mime xml）哈希固定。

自动解析版本的包若长期不重建，产物会停留在上次解析的版本；每月 1 号的全量定时
重建已覆盖这一点。若上游改动导致解析失败（仓库换名、资产命名变化等），构建会
报错终止——修 PKGBUILD 后 push 即可。

上游若移动旧 tag，"解析校验值"与"makepkg 下载"两次独立请求间存在 TOCTOU 窗口，
但校验不一致会直接构建失败，不会产出混版本产物；浮动源（`SKIP`）的包靠
`prepare()` 的结构检查兜底。

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
# 参照已有包的 PKGBUILD 编写：源码构建参照 wayland-pipewire-idle-inhibit-aur，
# 预编译重打包参照 we-layerd（deb）/ mark-shot（现成 pacman 包）；源码编译参照 noctalia-greeter（meson）
git add && git commit && git push   # 自动触发构建
```

验证一律以 CI 为准（构建 + `pacman -Qip` 元数据核验 + namcap），不在本地跑 makepkg——本仓库的意义就是本机零编译零依赖污染。

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
- 重打包预编译产物的包一律声明 `options=('!strip' '!debug')`：makepkg 默认 strip 会改动上游二进制，保真才能让产物与上游逐字节一致
- `build.conf` 只允许受控的变量赋值，工作流会校验包名、缓存路径、额外产物路径和 NDK 校验值；不要在配置中写 shell 表达式
- 构建完成后会强制检查至少生成一个包，并执行 `pacman -Qip`；默认执行 `namcap`。若某个大型预编译包不适合扫描，可在其 `PKGBUILD` 顶层声明 `namcap_check=false`，工作流会跳过该包的 namcap。检查失败会阻止上传产物

## Roadmap

- [x] 一期：增量构建（只构建变更的包）+ 并行 matrix + cargo 缓存
- [ ] 二期：nvchecker 自动检测上游发版并触发构建
- [ ] 三期：Releases 当私人 pacman 仓库（repo-add），实现 pacman -Syu 直接更新
