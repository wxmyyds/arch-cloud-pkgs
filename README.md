# arch-cloud-pkgs

个人 Arch Linux 包云端构建仓库：用 GitHub Actions 在 `archlinux` 容器里跑 `makepkg`，本机零编译零依赖污染。

## 已收录包

| 包名 | 上游 | 说明 |
|------|------|------|
| rtk-termux | [rtk-ai/rtk](https://github.com/rtk-ai/rtk) | 交叉编译的 Termux aarch64 版（上游只发 gnu/musl 预编译，没有 Bionic），版本构建时跟随最新 release。**不要在本机 Arch 上安装**，见下 |
| wechat | [腾讯微信](https://linux.weixin.qq.com/) | 腾讯官方微信 Linux x86_64 AppImage 重打包为原生包；仅支持 x86_64，版本自动跟随官网，ARM 版暂未接入 |
| google-chrome | [Google](https://www.google.com/chrome) | Google 官方 Chrome .deb 重打包；版本与 SHA256 从官方 apt 索引动态解析并由 makepkg 落地校验，打包逻辑对齐 AUR 同名包。`paru -Syu` 提示同版本"升级"时跳过 |
| rtk | [rtk-ai/rtk](https://github.com/rtk-ai/rtk) | LLM token 节省代理，Rust musl 静态单文件零依赖。行为仿照官方 install.sh（302 解析 tag + checksums.txt 动态校验 + CWE-22 防御），装到 `/usr/bin/rtk`；装完删除手动装的 `~/.local/bin/rtk` |
| clavis-shell | [StatIndet/quickshell](https://github.com/StatIndet/quickshell) | Clavis QML/Qt6 Shell；Actions 克隆上游 main 当前源码，先应用 `pkgs/clavis-shell/patches/*.patch`，再用上游 release.py 生成带日期版本的源码包和 PKGBUILD 后编译；AUR 运行库 libcava 与 qt6-m3shapes-git 也在同一云端任务源码构建并随 artifact 提供。需同时安装 key-cli artifact |
| key-cli | [StatIndet/key-cli](https://github.com/StatIndet/key-cli) | Clavis 配套 CLI 与系统指标服务；Actions 克隆上游 main 当前源码并云端编译。artifact 仅包含 key-cli 本体，不包含需单独授权的键盘/CPU 权限包 |
| neriplayer | [cwuom/NeriPlayer-Desktop](https://github.com/cwuom/NeriPlayer-Desktop) | 多源在线播放音乐客户端（Tauri 2 + Vue 3 + Rust 音频引擎，内置 FFmpeg 解码库）。上游已正式发 Release（tag 形如 `NeriPlayer-<sha7>.<MMDDHHMM>`，每个 Release 附 deb/rpm/AppImage 与 `SHA256SUMS.txt`）：本包取 `releases/latest` 的 tag，直接下载官方 amd64 deb 重打包，版本 = 应用版本 + 构建 epoch（`1.0.0.r<epoch>`）保持单调升级，每次上游发版自动跟随。deb 的 SHA256 直接取自该 Release 的 `SHA256SUMS.txt` 硬校验 |
| dux-files | [duxlabs-dev/dux-files](https://github.com/duxlabs-dev/dux-files) | Dux Labs 的 Wayland 文件管理器（Quickshell UI + Go daemon，UI 经 go:embed 内嵌进单文件二进制）。上游 release 直接发静态 linux-amd64/arm64 二进制与补全包，本包取 amd64 预编译产物重打包（不编译），桌面项/图标/polkit/D-Bus/portal/systemd 资产从同 tag raw 取源；版本自动跟随 `releases/latest`。运行时依赖 `quickshell`（官方仓库包）|

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

### Dux Files 安装

dux-files 是官方预编译二进制重打包（UI 已内嵌，单文件），运行时需要官方仓库的
`quickshell`（提供 `Quickshell.*` QML 模块与 `qs` 运行时）与 `qt6-declarative`，
由 pacman 依赖自动拉取。PDF 预览另需可选的 `qt6-webengine`。
与 AUR 的 `dux-files-bin` / `dux-files-git` 同装会争用 `/usr/bin/dfiles`，本包
已声明 `conflicts`，按 pacman 提示先卸载即可。systemd 用户单元需自行启用：
`systemctl --user enable --now dfiles`；作为默认文件管理器与 portal 后端也需
用户主动开启（`xdg-mime default dev.duxlabs.files.desktop inode/directory`、
`dfiles portal install`）。

### Clavis Shell 安装

Clavis 需要同时下载 arch-packages-clavis-shell 与 arch-packages-key-cli 两个 artifact，并将其中的 .pacman 文件放在同一目录后运行 `sudo pacman -U *.pacman`。CI 每次从两个上游仓库的 main 获取源码，对 clavis-shell 先应用 `pkgs/clavis-shell/patches/` 下的本地补丁，再通过上游 release.py 按构建日期生成版本并在云端编译（补丁因此被烘焙进源码包与安装包，桌面机无需任何手工改动）；Clavis artifact 另包含 CI 源码构建的 libcava、qt6-m3shapes-git。key-cli artifact 只含 CLI 本体，不含键盘/CPU 特权授权包；桌面机不运行 makepkg。

**当前本地补丁**（每个均为上游未修复时的临时 workaround，含背景、触发条件与移除方法，见 [`pkgs/clavis-shell/patches/README.md`](pkgs/clavis-shell/patches/README.md)）：

- `0001-bar-wheel-scroll-and-brightness-init.patch` —— Qt 6.12 任务栏亮度/麦克风滚轮失效（WheelHandler 被 RippleButton 吞事件）。
- `0002-idle-monitor-rebuild-workaround.patch` —— quickshell IdleMonitor 同地址销毁重建后 `isIdle` 失联（上游 [#938]/[#1097]），Clavis 自动锁屏/息屏/休眠永不触发，本补丁强制策略加载后再建通知并在配置变化时重建 monitor。

> 所有包的 artifact 统一**不含 `-debug` 调试符号包**：Arch 的 `makepkg.conf` 默认 `OPTIONS` 含 `debug`，源码构建类包会额外产出 `<pkg>-debug-*.pkg.tar.zst`（clavis-shell 约 18MB、key-cli 约 3.8MB），构建脚本在上传前统一删除（见 `.github/workflows/build.yml`）。

**前置坑：**

- `rtk-termux` 的 `arch=('aarch64')`，产物是 Android Bionic 二进制，**在 x86_64 主机上会被 pacman 的架构检查拦下**——这是刻意的保护，装进去会顶掉正常的 `/usr/bin/rtk`。
  给 Termux 用的话取 artifact 里的裸二进制 `rtk-aarch64-android` 即可，不需要走 pacman。
### 升级某个包
所有包的 `pkgver` 均在构建时自动解析上游最新版（release tag / apt 索引 / 官网页面 / 工作流构建产物），
**无需改任何文件**：上游发版后定时重建（每月 1 号）或手动 workflow_dispatch 指定包名
即取当时最新版。解析失败或版本号格式异常会直接报错终止构建，不会打出错误版本。

校验值的来源按包而异，均由 makepkg 下载后落地校验：

- `google-chrome`：版本与 SHA256 从官方 apt `Packages` 索引解析。
- `rtk`：deb/tarball 的 SHA256 从该 tag 的 `checksums.txt` 解析。
- `rtk-termux`：GitHub archive 源码 tarball 动态生成、官方不承诺内容寻址，
  `sha256sums` 为 `SKIP`；完整性由 `prepare()` 的体积下限 + `Cargo.toml`/`src/`
  结构检查（version 与 `pkgver` 对账）兜底。
- `wechat`：官网未提供固定 SHA-256，`sha256sums` 为 `SKIP`；完整性由
  `prepare()` 的体积下限、AppImage type-2 魔数和解包后的关键文件检查兜底。
  源文件名包含版本号，避免固定下载 URL 在 CI 的 `SRCDEST` 缓存中复用旧版本。
- `neriplayer`：上游已正式发 Release，每个 Release 附 `NeriPlayer_<ver>_amd64.deb`
  与 `SHA256SUMS.txt`（含该 deb 的 SHA256）、`build-metadata.txt`（含 `build_epoch`、
  `build_git_sha`、`release_tag`）。deb 的 SHA256 直接从 `SHA256SUMS.txt` 解析并硬
  校验，不再需要 `SKIP`；`LICENSE`（GPL-3.0）从该 Release 的 `build_git_sha` 对应
  raw 流式计算 SHA256。其余完整性由 `prepare()` 的体积下限 + ar 魔数 + deb 结构 +
  版本对账（control Version / deb 文件名 / `release_tag` 三方一致）+ 装包后 `ldd`
  兜底。早期版本经 nightly.link 取 artifact zip，zip 仅保留 3 天且无摘要，现已弃用。
- `dux-files`：release 资产自带 `dfiles-linux-amd64.gz.sha256` 与
  `dfiles-completions-<ver>.tar.gz.sha256`，直接解析为二进制/补全包的 SHA256；
  桌面项、图标、polkit、D-Bus、portal、systemd 单元、LICENSE、README 按同一 tag
  从仓库 raw 流式计算 SHA256。全部由 makepkg 落地校验，另有体积下限、ELF 魔数与
  `dfiles version` 的版本对账兜底。

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
# 参照已有包的 PKGBUILD 编写：源码构建参照 rtk（Rust）；预编译重打包参照 wechat（AppImage）/
# google-chrome（deb）/ neriplayer（release 里的官方 deb）/ dux-files（release 里的预编译二进制）；
# 交叉编译参照 rtk-termux（cargo-ndk + NDK）
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
- [ ] 三期：Git Tag + Release 作为分发通道（每个版本一个 tag `pkg@ver-rel`，Release
  描述存该版本全部元数据，Assets 只放安装包），由轻量包管理器读取并安装

> 三期不使用 pacman 数据库（`repo-add` / `cloud.db`）。版本即 Git Tag，元数据即
> Release 描述，因此不存在跨包的共享索引，发布过程无需串行化。
