# 维护指南

## 手动构建

在 GitHub Actions 的 **Build packages** workflow 中指定包名。也可从构建运行下载该包的 `arch-packages-<包名>` artifact；artifact 保留 30 天。成功构建后，包还会发布到 GHCR OCI。

推送包目录或相关构建文件的变更会自动构建受影响的包。构建使用 `archlinux:base-devel`，并运行包元数据检查和 namcap；验证以 Actions 为准，不在本机运行 `makepkg`。

## 上游更新

`.github/nvchecker.toml` 声明各包的上游版本来源，`.github/workflows/check-updates.yml` 每日 `05:23 UTC` 运行检查。有更新时只触发对应包构建；版本基线仅在所有条目都成功取到版本后推进。

手动运行检查 workflow 时 `dry_run` 默认开启，只报告结果。正式构建时，版本和校验值仍由对应 `PKGBUILD` 解析并验证；nvchecker 结果只用于发现变化。

## 新增或修改包

1. 在 `pkgs/<包名>/` 添加 `PKGBUILD`；需要构建缓存或额外依赖时添加 `build.conf`。
2. 更新 [PACKAGES.md](../PACKAGES.md)；若需通过 cpkg 发布，同时更新 [`cpkg/packages.json`](../cpkg/packages.json)。
3. 若要启用自动上游检测，在 `.github/nvchecker.toml` 添加同名条目，并更新 `.github/nvchecker/oldver.json`。
4. 推送后检查 Actions 构建、包元数据检查和发布状态。

`build.conf` 支持的字段及工具链配置见 [.github/scripts/prepare-toolchain.sh](../.github/scripts/prepare-toolchain.sh)。workflow 脚本应通过 `env:` 接收 GitHub 上下文，不要将 `${{ }}` 直接插入 shell 命令。

## 相关文档

- [包清单](../PACKAGES.md)
- [cpkg 使用说明](USAGE.md)
- [Clavis Shell 补丁维护](../pkgs/clavis-shell/patches/README.md)
