# arch-cloud-pkgs

个人 Arch Linux 包云端构建仓库：用 GitHub Actions 在 `archlinux` 容器里跑 `makepkg`，本机零编译零依赖污染。

## 已收录包

当前维护的包、上游来源与打包说明见[包清单](PACKAGES.md)。

## 使用方法

### 触发构建
1. 推送代码后自动触发；或到 **Actions → Build packages → Run workflow** 手动触发
2. 上游发版后可手动触发对应包的构建；二期自动检测目前默认禁用
3. 等待构建完成（见下方"构建加速"）
4. 到该次运行页面底部下载对应包的 artifact `arch-packages-<包名>`；每个包单独一个文件，互不影响
> Actions artifact 保留 30 天（`retention-days: 30`）；构建成功后同一包也会发布到 GHCR OCI。
> 新建的 GHCR package 需要先设为 public，匿名客户端才能读取 manifest 和下载包。

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

Clavis 需要同时安装 `clavis-shell`、`libcava`、`qt6-m3shapes-git` 与 `key-cli` 四个包。CI 为三个依赖建立独立构建 job 与 artifact；构建 clavis-shell 时下载并安装它们，不重复编译。手动安装时，将四个 artifact 中的 .pacman 文件放在同一目录后运行 `sudo pacman -U *.pacman`；使用 cpkg 时运行 `cpkg -S clavis-shell`，客户端会自动解析并安装另外三个依赖。CI 每次从两个上游仓库的 main 获取 Clavis/key-cli 源码，对 clavis-shell 先应用 `pkgs/clavis-shell/patches/` 下的本地补丁，再通过上游 release.py 按构建日期生成版本并云端编译。key-cli artifact 只含 CLI 本体，不含键盘/CPU 特权授权包；桌面机不运行 makepkg。

**当前本地补丁**（每个均为上游未修复时的临时 workaround，含背景、触发条件与移除方法，见 [`pkgs/clavis-shell/patches/README.md`](pkgs/clavis-shell/patches/README.md)）：

- `0001-bar-wheel-scroll-and-brightness-init.patch` —— Qt 6.12 任务栏亮度/麦克风滚轮失效（WheelHandler 被 RippleButton 吞事件）。
- `0002-idle-monitor-rebuild-workaround.patch` —— quickshell IdleMonitor 同地址销毁重建后 `isIdle` 失联（上游 [#938]/[#1097]），Clavis 自动锁屏/息屏/休眠永不触发，本补丁强制策略加载后再建通知并在配置变化时重建 monitor。

> 所有包的 artifact 统一**不含 `-debug` 调试符号包**：Arch 的 `makepkg.conf` 默认 `OPTIONS` 含 `debug`，源码构建类包会额外产出 `<pkg>-debug-*.pkg.tar.zst`（clavis-shell 约 18MB、key-cli 约 3.8MB），构建脚本在上传前统一删除（见 `.github/workflows/build.yml`）。

### 升级某个包
所有包的 `pkgver` 均在构建时自动解析上游最新版（release tag / apt 索引 / 官网页面 / 工作流构建产物），
**无需改任何文件**：手动 workflow_dispatch 指定包名即可在构建时解析上游最新版；仓库变更
也会自动触发相关包构建。二期上游更新检测默认禁用，启用后可自动触发构建。解析失败或
版本号格式异常会直接报错终止构建，不会打出错误版本。

校验值的来源按包而异，均由 makepkg 下载后落地校验：

- `google-chrome`：版本与 SHA256 从官方 apt `Packages` 索引解析。
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

自动解析版本只在构建时刷新；若上游改动导致解析失败（仓库换名、资产命名变化等），
构建会报错终止——修 PKGBUILD 后 push 即可。Arch 滚动更新若造成包的 ABI 依赖失效，
需手动触发对应包重建；本工作流不再定期全量重建。

上游若移动旧 tag，"解析校验值"与"makepkg 下载"两次独立请求间存在 TOCTOU 窗口，
但校验不一致会直接构建失败，不会产出混版本产物；浮动源（`SKIP`）的包靠
`prepare()` 的结构检查兜底。

### 上游发版自动检测（二期）

`.github/nvchecker.toml` 用 [nvchecker](https://github.com/lilydjwg/nvchecker)
逐包声明"上游版本怎么取"，`.github/workflows/check-updates.yml` 跑检测，有变化时
调用 `gh workflow run build.yml -f packages=<包名>` 触发构建。每个条目的表名必须
等于 `pkgs/` 下的目录名，检测结果只在包目录真实存在时才作为构建目标。

来源按包性质选择：

| 包 | 来源 | 说明 |
|----|------|------|
| `google-chrome` | `apt` | 直接读官方 apt 索引，`strip_release` 去掉 Debian revision，与 PKGBUILD 的 `${_debver%-*}` 一致 |
| `wechat` | `regex` | 官网 SSR 的 `class="main-section__bd-version"` 块；用 class 前缀锚定，避免误命中同名 CSS 文本 |
| `neriplayer` / `dux-files` | `github` + `use_latest_release` | 跟踪 release tag |
| `clavis-shell` / `key-cli` | `github` + `branch = "main"` | 两端都是"克隆 main 当前源码"构建，发版覆盖不全，故跟踪提交时间 |
| `libcava` / `qt6-m3shapes-git` | `aur` | 跟踪 AUR PKGBUILD 版本；任一包更新只触发对应独立构建 |

只把检测结果当**变更信号**：最终 `pkgver` 与校验值仍由各 PKGBUILD 构建时自行解析，
因此 dux-files 的 `v2.0.0` 这类带前缀的取值不需要与 pkgver 同形。

基线放在 `.github/nvchecker/oldver.json`（提交进仓库），`newver.json` 每轮生成且
已 gitignore。检测脚本只有在**全部条目都取到版本**时才允许推进基线，避免一次网络
抽风把基线写成空值后该包再也检测不到更新；某条目取不到时会在日志与 job summary
里以 `missing` 列出，但不阻断其余包的检测。

**默认禁用**：`check-updates.yml` 的 `schedule` 已注释，手动触发时 `dry_run` 默认
`true`（只检测、只写日志与 summary）。上线时取消 `schedule` 注释即可全天候自动
检测；定时触发不受 `dry_run` 约束。

## 构建加速

- **只构建变更的包**：push 时按 diff 选出改动涉及的包，也可手动指定单个包
- **并行构建**：多个包同时变更时，每个包一个独立 job（matrix）并行执行，互不影响；产物也按包分别上传
- **缓存分层**：
  - 工具链层（跨包共享）：cargo registry/git、`SRCDEST` 源码下载副本（以及启用时的 NDK）——都是内容寻址的只读资源
  - 包层（按包隔离）：`cache_dirs` 声明的编译目录（Rust 约定 `target/`），key 含包名 + PKGBUILD + build.conf + 工具链版本 + 架构
  - `src/`（解压后的源码现场）**不缓存**，每次由 makepkg 从 `SRCDEST` 重新获取，避免"源码与中间产物混杂"的脏缓存
  - 缓存受 GitHub 10GB/仓库 配额限制，旧版本缓存会自动淘汰
- **自动取消**：同一分支的新提交会取消仍在跑的旧构建，避免浪费 runner

## 添加新包

```bash
mkdir pkgs/<新包名>
# 参照已有包的 PKGBUILD 编写：源码构建参照 clavis-shell（CI 克隆上游 main）；
# 预编译重打包参照 wechat（AppImage）/ google-chrome（deb）/
# neriplayer（release 里的官方 deb）/ dux-files（release 里的预编译二进制）
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
  所以 `CARGO_HOME`、`RUSTUP_HOME`（以及 NDK 路径，若启用）在 PKGBUILD 里都用同样的
  默认值又兜底了一遍——**改路径时 `prepare-toolchain.sh` 和对应 PKGBUILD 要同步改**
- 交叉编译包只声明 `rustup`，不要和 `rust` 同时写进 `makedepends`：
  Arch 的 rustup 包 `conflicts=('cargo' 'rust' 'rustfmt')`，两者互斥
- NDK / 交叉编译这套能力仍保留在 `prepare-toolchain.sh` 与 `build.conf` 里，但当前
  没有包在用（原先唯一的使用者 `rtk-termux` 已移除）
- 工作流里所有 GitHub 上下文都经 `env:` 传入，不在 `run:` 里直接写 `${{ }}`——
  后者是 shell 解析前的纯文本替换，`workflow_dispatch` 输入里带个分号就能执行任意命令
- 重打包预编译产物的包一律声明 `options=('!strip' '!debug')`：makepkg 默认 strip 会改动上游二进制，保真才能让产物与上游逐字节一致
- `build.conf` 只允许受控的变量赋值，工作流会校验包名、缓存路径、额外产物路径和 NDK 校验值；不要在配置中写 shell 表达式
- 构建完成后会强制检查至少生成一个包，并执行 `pacman -Qip`；默认执行 `namcap`。若某个大型预编译包不适合扫描，可在其 `PKGBUILD` 顶层声明 `namcap_check=false`，工作流会跳过该包的 namcap。检查失败会阻止上传产物

## Roadmap

- [x] 一期：增量构建、并行 matrix 与 cargo 缓存
- [x] 二期：nvchecker 上游版本检测与构建触发已实现；定时 schedule 默认关闭，手动触发默认为 dry run
- [x] 三期：Rust 客户端 `cpkg` 通过 GHCR OCI 查询、下载和校验云端包，再交由 pacman 安装；不使用 pacman 仓库数据库

## cpkg 使用说明

`cpkg` 是 GHCR 云端包客户端，命令格式统一采用 pacman 的选项语法，不提供 `ls`、`install` 等子命令或包名简写。可用包及上游信息见[包清单](PACKAGES.md)，客户端清单和云端依赖关系见 [`cpkg/packages.json`](cpkg/packages.json)。CI 会执行格式检查、单元测试、Clippy 和 release 构建，并上传 30 天有效的 x86_64 二进制 artifact。

### 命令

| 命令 | 用途 |
|------|------|
| `cpkg -Sl` | 列出可用云端包及最新版本 |
| `cpkg -Si <包>` | 查看包详情和 OCI 摘要信息 |
| `cpkg -Ss <正则>` | 按包名或简介搜索，支持正则 |
| `cpkg -Qu [包]` | 检查已安装云端包是否有更新；不指定包时检查全部 |
| `cpkg -S <包...>` | 安装指定云端包及其云端依赖 |
| `cpkg -S <包>=<版本>` | 安装指定版本 |
| `cpkg -Syu` | 升级所有已安装且有新版本的云端包 |

`cpkg -Syu` 使用 pacman 的 sync/refresh/sysupgrade 操作语法，但只处理 cpkg 管理的 GHCR 包，不运行 pacman 仓库升级。长选项形式为 `cpkg --sync --refresh --sysupgrade`。其他 pacman 选项可直接放在命令中，例如：

```sh
cpkg -S clavis-shell key-cli --noconfirm
cpkg -S clavis-shell=1.2.3-1 --noconfirm
cpkg -Syu --noconfirm
```

以其他 pacman 操作进行的本地查询、移除等操作直接运行 pacman，例如 `sudo pacman -Rns key-cli`；cpkg 不处理也不透传这些命令。云端包安装会下载 OCI layer、校验 SHA-256 后调用 `pacman -U`；官方仓库依赖仍由 pacman 处理。Clavis 的云端依赖为 `key-cli`、`libcava` 和 `qt6-m3shapes-git`，使用 `cpkg -S clavis-shell` 会自动安装依赖。

`cpkg` 不使用 `repo-add` 或 `cloud.db`。OCI 元数据查询只读取 manifest，不下载包文件。卸载行为由用户选择的 pacman 选项决定；例如 `-Rns` 会递归移除不再需要的依赖并删除 `.pacsave` 配置文件。新建 GHCR package 后需将 visibility 设为 public，客户端才能匿名查询和下载。同一版本 tag 不允许覆盖，重打包时需先更新 pkgrel。

客户端源码按 MIT License 发布（见 [`cpkg/LICENSE`](cpkg/LICENSE)）；此授权仅适用于 `cpkg/` 客户端，不改变仓库中各上游软件与打包资产原有的许可证。
