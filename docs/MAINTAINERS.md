# 维护指南

## 手动构建

在 GitHub Actions 的 **Build packages** workflow 中指定包名。也可从构建运行下载该包的 `arch-packages-<包名>` artifact；artifact 保留 30 天。

特性分支只上传 artifact，不发布 GHCR。手动构建的 `publish` 默认关闭；仅在 `main` 明确开启它才会发布。
`main` 的 push 构建仍在成功后自动发布；特性分支须手动选择包名触发，不构建无关包。

推送包目录或相关构建文件的变更会自动构建受影响的包。构建使用 `archlinux:base-devel`，并运行包元数据检查和 namcap；验证以 Actions 为准，不在本机运行 `makepkg`。

## 上游更新

`.github/nvchecker.toml` 声明各包的上游版本来源，`.github/workflows/check-updates.yml` 每日 `05:23 UTC` 运行检查。有更新时只触发对应包构建；版本基线仅在所有条目都成功取到版本后推进。

手动运行检查 workflow 时 `dry_run` 默认开启，只报告结果。正式构建时，版本和校验值仍由对应 `PKGBUILD` 解析并验证；nvchecker 结果只用于发现变化。

## 新增或修改包

1. 在 `pkgs/<包名>/` 添加 `PKGBUILD`；需要构建缓存或额外依赖时添加 `build.conf`。

   AUR 包（`libcava`、`qt6-m3shapes-git`）由 `build-aur-package.sh` 现克隆 AUR 的 PKGBUILD 构建，仓库内的 `PKGBUILD` 仅为占位。若要重编并发布新产物（例如运行库 ABI/soname 变化），在 `pkgs/<包名>/pkgrel` 写入目标 pkgrel：脚本会把它覆盖到克隆出的 PKGBUILD 上。AUR 上游 pkgrel 恒为 1，不抬的话会命中已发布的不可变 OCI tag，发布步骤只告警不推送。发布后将新版本同步到 `.github/nvchecker/oldver.json` 的对应条目。
2. 更新 [PACKAGES.md](../PACKAGES.md)；若需通过 cpkg 发布，同时更新 [`cpkg/packages.json`](../cpkg/packages.json)。
3. 若要启用自动上游检测，在 `.github/nvchecker.toml` 添加同名条目，并更新 `.github/nvchecker/oldver.json`。
4. 推送后检查 Actions 构建、包元数据检查和发布状态。

`build.conf` 支持的字段及工具链配置见 [.github/scripts/prepare-toolchain.sh](../.github/scripts/prepare-toolchain.sh)。workflow 脚本应通过 `env:` 接收 GitHub 上下文，不要将 `${{ }}` 直接插入 shell 命令。

## 相关文档

Clavis 歌词补丁使用公开版本化自定义 `org.cirrus.Lyrics1` D-Bus 接口；协议字段见
[Cirrus LYRICS_DBUS.md](https://github.com/wxmyyds/CirrusMusic/blob/feat/bili-source-ui-lyrics/LYRICS_DBUS.md)。
更新补丁时同步检查接口版本、逐字数据保留、关闭导出清理与无鼠标交互的自动换句。
桌面验证需安装两个新包并完整重启进程；QML 热重载不能替换已加载的原生歌词库。

- [包清单](../PACKAGES.md)
- [cpkg 使用说明](USAGE.md)
- [Clavis Shell 补丁维护](../pkgs/clavis-shell/patches/README.md)
