# 维护指南

## 手动构建

在 GitHub Actions 的 **Build packages** workflow 中指定包名。验证构建的 `arch-packages-<包名>` artifact 保留 1 天，请及时下载。主线完成全部构建和 GHCR 发布后，独立清理 job 删除本次运行的中转包；失败或取消的运行保留产物至到期，便于诊断和重跑。

`.github/scripts/cleanup-run-artifacts.sh` 仅用于本仓库，固定清理当前运行的 `arch-packages-*` artifact，并拒绝在其他仓库运行。清理在依赖安装、全部构建和发布成功后执行，不删除 GHCR 包、Release 资产或缓存。

特性分支只上传 artifact，不发布 GHCR。手动构建的 `publish` 默认关闭；仅在 `main` 明确开启它才会发布。
`main` 的 push 构建仍在成功后自动发布；特性分支须手动选择包名触发，不构建无关包。

推送包目录或相关构建文件的变更会自动构建受影响的包。构建使用 `archlinux:base-devel`，并运行包元数据检查和 namcap；收集、依赖安装和 artifact 均使用标准 `.pkg.tar.zst`，不再改名。验证以 Actions 为准，不在本机运行 `makepkg`。该操作需要通过 GitHub Actions 执行。

`Build cpkg client` 在 x86_64 和 aarch64 原生 Ubuntu runner 上执行 Rust 格式化、单元测试、Clippy 和编译，并分别上传保留 1 天的客户端二进制。`pkgs/cpkg/PKGBUILD` 声明这两个架构，但 `Build packages` 目前仍只有 x86_64 Arch 包构建；aarch64 客户端先通过二进制 artifact 提供，不能据此宣称云端 cpkg 自更新包或其他第三方包已经支持 ARM。

## 上游更新

`.github/nvchecker.toml` 声明各包的上游版本来源，`.github/workflows/check-updates.yml` 每日 `05:23 UTC` 运行检查。有更新时只触发对应包构建；版本基线仅在所有条目都成功取到版本后推进。

手动运行检查 workflow 时 `dry_run` 默认开启，只报告结果；更新检测报告保留 3 天。正式构建时，版本和校验值仍由对应 `PKGBUILD` 解析并验证；nvchecker 结果只用于发现变化。

## 新增或修改包

1. 本仓库负责构建的包在 `pkgs/<包名>/` 添加 `PKGBUILD`；需要构建缓存或额外依赖时添加 `build.conf`。上游已经构建并提供完整 Arch 包的外部生产者使用下方统一发布接口；Cirrus 仅走此接口，不新增重复的 PKGBUILD、构建或发布工作流。

   AUR 包（`libcava`、`qt6-m3shapes-git`）由 `build-aur-package.sh` 现克隆 AUR 的 PKGBUILD 构建，仓库内的 `PKGBUILD` 仅为占位。若要重编并发布新产物（例如运行库 ABI/soname 变化），在 `pkgs/<包名>/pkgrel` 写入目标 pkgrel：脚本会把它覆盖到克隆出的 PKGBUILD 上。AUR 上游 pkgrel 恒为 1，不抬的话会命中已发布的不可变 OCI tag；摘要不同会使发布明确失败，相同摘要的重试才幂等跳过。发布后将新版本同步到 `.github/nvchecker/oldver.json` 的对应条目。
2. 更新 [PACKAGES.md](../PACKAGES.md)；若需通过 cpkg 发布，同时更新 [`cpkg/packages.json`](../cpkg/packages.json)。
3. 若要启用自动上游检测，在 `.github/nvchecker.toml` 添加同名条目，并更新 `.github/nvchecker/oldver.json`。
4. 推送后检查 Actions 构建、包元数据检查和发布状态。

`build.conf` 支持的字段及工具链配置见 [.github/scripts/prepare-toolchain.sh](../.github/scripts/prepare-toolchain.sh)。workflow 脚本应通过 `env:` 接收 GitHub 上下文，不要将 `${{ }}` 直接插入 shell 命令。

## 统一 OCI 发布与外部生产者

唯一发布入口为 `.github/scripts/publish-oci-package.sh <实际包路径> [<实际包路径> ...]`。不再支持 `PACKAGE_FILE` 环境变量。必需环境为 `PACKAGE_NAME`、`GITHUB_REPOSITORY`（源代码所属的调用仓库）；`PUBLISH` 只接受 `true` / `false`，默认 `true`。仅正式发布还需 `GITHUB_ACTOR`、`GITHUB_TOKEN` 和 `curl`；调用 job 授予 `contents: read`、`packages: write`，使用调用仓库自带的 `GITHUB_TOKEN`，无需跨仓库 PAT 或新建 secrets。

发布器使用 Bash、`jq`、`bsdtar`、`zstd` 和标准 coreutils。每个输入必须是实际 zstd 压缩的 `.pkg.tar.zst`，`.PKGINFO` 的包名须等于 `PACKAGE_NAME`，完整 `pkgver`（含 epoch/pkgrel）必须一致，且每个架构只能提供一个包。原生架构支持 `x86_64` / `aarch64`；`any` 只允许单文件发布。会计算每个包及生成元数据的 SHA-256 和大小。

- 单文件仍以现有的 `v<编码pkgver>` tag 发布原生 OCI manifest，保留已有 cpkg 自更新路径。
- 多文件先按摘要上传各架构 leaf manifest，再以相同版本 tag 发布 OCI index；描述符使用 `linux/amd64` 与 `linux/arm64`，不创建额外可见的架构版本 tag。
- 版本 tag 已存在时，只有根 manifest/index 摘要完全相同才可幂等跳过；任何不同摘要（包括架构集合或元数据变化）都失败，必须抬版本或 pkgrel。
- `PUBLISH=false` 完成全部包验证、leaf/index 生成和 SHA-256 计算，输出一行 JSON 验证报告后退出；不访问网络、不请求 token、不需要认证。仓库现有构建流程在不发布时也执行这个相同入口。

Cirrus 的两个 Arch 包只在 `wxmyyds/CirrusMusic` 的 Release 中构建。本仓库提供统一发布器，当前暂不收录 Cirrus 的 `cpkg/packages.json` 目录条目；不添加 `pkgs/cirrus/PKGBUILD`、nvchecker 条目或第二套发布工作流。Cirrus Release 用 `actions/checkout` 检出 `repository: wxmyyds/arch-cloud-pkgs`、`ref: main`、`path: build/cpkg-repository`，其 `packaging/linux/arch/cpkg/` 包装器把两个已构建的实际路径作为位置参数交给本发布器：

```bash
PACKAGE_NAME=cirrus PUBLISH=false \
  bash build/cpkg-repository/.github/scripts/publish-oci-package.sh \
    "$x86_64_package" "$aarch64_package"
```

上例在 Actions 中由调用方给两个变量赋实际包路径；`GITHUB_REPOSITORY` 保持调用方 CirrusMusic，生成目标为 `ghcr.io/wxmyyds/cirrus`。验证时保持 `PUBLISH=false`；正式 Release 才设为 `true` 并传入 job 的 `GITHUB_TOKEN`。若包装器提供 `cpkg_ref`，其默认必须为 `main`，仅 `publish=false` 时允许特性分支，限制由调用方 Release/包装器执行。先合入发布器，再由上游完成首次正式发布并确认 GHCR 可匿名拉取，最后单独添加 `cpkg/packages.json` 与包清单条目；届时用户才可通过 `cpkg -S cirrus` 获取本机架构。未首发的包不提前进入可安装目录，避免查询或升级检查失败。

验证必须在 Actions 覆盖：两架构真实包的无凭据验证、单文件 manifest 与多文件 index 生成、版本编码、压缩/名称/版本/架构重复拒绝、客户端 index 选择及错误拒绝，以及包安装链路。Rust 格式化/测试/Clippy/编译和任何包构建均不在本机执行；该操作需要通过 GitHub Actions 执行。正式 GHCR 写入还需检验调用仓库 `GITHUB_TOKEN` 的包权限、同摘要重试与不同摘要拒绝，不能以本地静态检查代替。

## 相关文档

Clavis 从自维护 [wxmyyds/quickshell](https://github.com/wxmyyds/quickshell) 的 `main` 构建，
源码修改直接提交 fork，不再维护或叠加 `pkgs/clavis-shell/patches`。`key-cli` 仍从 `StatIndet/key-cli` 构建。
自动更新检测同样跟踪 fork 的 `main`；更换来源时保留当前相同提交的版本基线，不伪造尚未合并的版本。

手动构建 `clavis-shell` 可设置 `clavis_ref` 为 fork 特性分支并保持 `publish=false`，
用于在 fork PR 合并前验证源码、测试及包；该输入不改变正式跟踪源，非 main 源码禁止发布。
应先验证并合并 fork PR，再合并本仓库来源迁移 PR，正式构建仍使用 fork 的 `main`。

歌词使用 Cirrus 上游 Unix socket v1 协议；字段和发现清单见
[Cirrus now-playing](https://github.com/wxmyyds/CirrusMusic/tree/master/tools/now-playing)。
验证需覆盖逐字数据保留、B 站字幕、关闭导出清理和无鼠标交互的自动换句。
只有发现清单会从运行时目录读取，歌词正文通过 socket 获取，不使用歌词文件或联网脚本。
桌面验证需安装 Cirrus 和 Clavis 两个新包并重启进程；QML 热重载不能替换原生歌词库。

- [包清单](../PACKAGES.md)
- [cpkg 使用说明](USAGE.md)
- [Clavis Shell fork](https://github.com/wxmyyds/quickshell)
