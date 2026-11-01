# cpkg 使用

`cpkg` 管理本仓库发布到 GHCR 的云端包。它不替代 pacman：官方仓库依赖和本地查询、移除操作仍由 pacman 处理。

## 命令

| 命令 | 作用 |
|------|------|
| `cpkg -Sl` | 列出云端包 |
| `cpkg -Si <包>` | 查看包信息 |
| `cpkg -Ss <正则>` | 搜索包名或简介 |
| `cpkg -Qu [包]` | 检查已安装包更新；省略包名时检查全部 |
| `cpkg -S <包...>` | 安装包及云端依赖 |
| `cpkg -S <包>=<版本>` | 安装指定版本 |
| `cpkg -Syu` | 更新已安装的云端包（包含 cpkg 自身），不执行官方仓库升级 |

安装示例：

```sh
cpkg -S clavis-shell
cpkg -S clavis-shell=1.2.3-1
cpkg -Syu
```

安装前会显示事务包和总下载大小，并询问 `[Y/n]`；回车继续，输入 `n` 取消。确认后每个包显示下载进度、速度和预计剩余时间。`--noconfirm` 跳过 cpkg 确认，pacman 仍按自身设置处理安装确认。

普通用户运行 cpkg 即可：包下载并校验后，cpkg 会通过 sudo 执行 `pacman -U`。查询、下载和校验不提权。卸载等本地操作直接运行 pacman，例如 `sudo pacman -Rns key-cli`；cpkg 不透传这些操作。

## 安装方式

**通过 GHCR**：客户端会解析云端依赖并安装，例如 `cpkg -S clavis-shell` 会一并处理 `key-cli`、`libcava` 和 `qt6-m3shapes-git`。

**通过 Actions artifact**：从对应构建运行下载包文件，然后安装：

```sh
sudo pacman -U *.pacman
```

手动安装 Clavis 时需要同时下载其四个包的 artifact。`key-cli` 包只含 CLI，不含需单独授权的键盘/CPU 权限包。

## 包特定说明

- `dux-files` 运行时依赖官方仓库的 `quickshell` 和 `qt6-declarative`；PDF 预览可选装 `qt6-webengine`。用户服务可用 `systemctl --user enable --now dfiles` 启用。
- `wechat` 目前只提供 x86_64 包，由官方 AppImage 重打包；不需要 FUSE。
- 客户端语言按 `LC_ALL`、`LC_MESSAGES`、`LANG` 顺序选择；简体中文 locale 显示中文，其他情况显示英文。

完整包列表见 [PACKAGES.md](../PACKAGES.md)。
