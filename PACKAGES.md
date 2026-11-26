# 已收录包

本清单记录仓库当前维护的包、上游来源和打包方式。`cpkg` 云端可查询包与依赖关系以 [`cpkg/packages.json`](cpkg/packages.json) 为准。

Cirrus 的双架构外部发布接口已准备好，但它尚未正式首发，目前不加入可安装目录。待 CirrusMusic 上游 Release 通过本仓库唯一发布器上传实际包、并确认 GHCR 可匿名拉取后，再单独加入目录。本仓库不重建 Cirrus，也不创建 `pkgs/cirrus/PKGBUILD`。

| 包名 | 上游 | 说明 |
|------|------|------|
| cpkg | [本仓库](https://github.com/wxmyyds/arch-cloud-pkgs) | GHCR 云端包客户端；由本仓库从当前提交构建并发布。用 `cpkg -S cpkg` 安装，之后可用 `cpkg -Syu` 自更新。目前云端包为 x86_64；客户端 Actions 另提供 aarch64 原生二进制 artifact |
| wechat | [腾讯微信](https://linux.weixin.qq.com/) | 腾讯官方微信 Linux x86_64 AppImage 重打包为原生包；仅支持 x86_64，版本自动跟随官网。新版 AppImage 改为 AppRun + `wechat-payload.tar` + 摘要清单结构，PKGBUILD 校验 payload SHA-256 后解包到 `/opt/wechat`，不依赖 FUSE、首次 root 安装、AppArmor 或 SUID sandbox |
| google-chrome | [Google](https://www.google.com/chrome) | Google 官方 Chrome .deb 重打包；版本与 SHA256 从官方 apt 索引动态解析并由 makepkg 落地校验，打包逻辑对齐 AUR 同名包。`paru -Syu` 提示同版本“升级”时跳过 |
| clavis-shell | [wxmyyds/quickshell](https://github.com/wxmyyds/quickshell) | 自维护 Clavis fork；Actions 从 fork 的 main 源码生成带日期版本的源码包与 PKGBUILD 后编译，不再应用打包补丁。手动 `clavis_ref` 可验证 fork 特性分支且仅上传 artifact。需一并安装 `key-cli`、`libcava` 与 `qt6-m3shapes-git`；通过 Cirrus 上游 Unix socket v1 接口获取歌词与时序，其他播放器可提供 MPRIS `xesam:asText` 纯文本；歌词区仅显示并自动换句，不读歌词文件/缓存或联网搜索 |
| libcava | [AUR](https://aur.archlinux.org/packages/libcava) | Cava shared library；从 AUR PKGBUILD 云端构建，Clavis Shell 运行库。重编需抬 `pkgs/libcava/pkgrel`（AUR 的 pkgrel 恒为 1，否则不同摘要命中已存在的不可变 OCI tag 会失败） |
| qt6-m3shapes-git | [AUR](https://aur.archlinux.org/packages/qt6-m3shapes-git) | Qt 6 Material 3 Expressive shapes；从 AUR PKGBUILD 云端构建，Clavis Shell 运行库 |
| key-cli | [StatIndet/key-cli](https://github.com/StatIndet/key-cli) | Clavis 配套 CLI 与系统指标服务；Actions 克隆上游 main 当前源码并云端编译。artifact 仅包含 key-cli 本体，不包含需单独授权的键盘/CPU 权限包 |
| neriplayer | [cwuom/NeriPlayer-Desktop](https://github.com/cwuom/NeriPlayer-Desktop) | 多源在线播放音乐客户端（Tauri 2 + Vue 3 + Rust 音频引擎，内置 FFmpeg 解码库）。跟踪 `NeriPlayer-<appver>` 最新 Release，下载官方 amd64 deb 重打包；tag、资产名、metadata 应用版本与 deb control 必须一致，版本保留构建 epoch，SHA256 从 `SHA256SUMS.txt` 校验 |
| dux-files | [duxlabs-dev/dux-files](https://github.com/duxlabs-dev/dux-files) | Dux Labs 的 Wayland 文件管理器（Quickshell UI + Go daemon，UI 经 go:embed 内嵌）。取官方预编译 linux-amd64 二进制重打包；运行时依赖官方仓库的 `quickshell` |
