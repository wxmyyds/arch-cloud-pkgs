# arch-cloud-pkgs

Arch Linux 包的云端构建与分发仓库。GitHub Actions 在 Arch 容器中构建包；用户可下载构建产物，或通过 `cpkg` 从 GHCR 安装。

## 使用

- [云端包清单](PACKAGES.md)
- [cpkg 命令与安装说明](docs/USAGE.md)
- [Actions 手动构建与下载 artifact](docs/MAINTAINERS.md#手动构建)

快速安装示例：

```sh
cpkg -Sl
cpkg -S clavis-shell
cpkg -Syu
```

## 自动化

- 推送包或构建配置变更时，Actions 只构建受影响的包；也可手动指定包名。
- 每日 `05:23 UTC` 检查上游版本，有变化时触发对应包构建。
- 成功构建的包上传为保留 30 天的 Actions artifact，并发布到 GHCR OCI。

## 维护

- [构建、上游检测与新增包流程](docs/MAINTAINERS.md)
- [Clavis Shell 临时补丁说明](pkgs/clavis-shell/patches/README.md)

构建与发布状态以 GitHub Actions 为准；本仓库不要求在本机运行 `makepkg`。
