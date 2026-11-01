# 已收录包

本清单记录仓库当前维护的包、上游来源和打包方式。`cpkg` 云端可查询包与依赖关系以 [`cpkg/packages.json`](cpkg/packages.json) 为准。

| 包名 | 上游 | 说明 |
|------|------|------|
| cpkg | [本仓库](https://github.com/wxmyyds/arch-cloud-pkgs) | GHCR 云端包客户端；由本仓库从当前提交构建并发布。用 `cpkg -S cpkg` 安装，之后可用 `cpkg -Syu` 自更新 |
| wechat | [腾讯微信](https://linux.weixin.qq.com/) | 腾讯官方微信 Linux x86_64 AppImage 重打包为原生包；仅支持 x86_64，版本自动跟随官网，ARM 版暂未接入 |
| google-chrome | [Google](https://www.google.com/chrome) | Google 官方 Chrome .deb 重打包；版本与 SHA256 从官方 apt 索引动态解析并由 makepkg 落地校验，打包逻辑对齐 AUR 同名包。`paru -Syu` 提示同版本“升级”时跳过 |
| clavis-shell | [StatIndet/quickshell](https://github.com/StatIndet/quickshell) | Clavis QML/Qt6 Shell；Actions 克隆上游 main 当前源码，先应用 `pkgs/clavis-shell/patches/*.patch`，再用上游 release.py 生成带日期版本的源码包和 PKGBUILD 后编译。需一并安装 `key-cli`、`libcava` 与 `qt6-m3shapes-git` |
| libcava | [AUR](https://aur.archlinux.org/packages/libcava) | Cava shared library；从 AUR PKGBUILD 云端构建，Clavis Shell 运行库 |
| qt6-m3shapes-git | [AUR](https://aur.archlinux.org/packages/qt6-m3shapes-git) | Qt 6 Material 3 Expressive shapes；从 AUR PKGBUILD 云端构建，Clavis Shell 运行库 |
| key-cli | [StatIndet/key-cli](https://github.com/StatIndet/key-cli) | Clavis 配套 CLI 与系统指标服务；Actions 克隆上游 main 当前源码并云端编译。artifact 仅包含 key-cli 本体，不包含需单独授权的键盘/CPU 权限包 |
| neriplayer | [cwuom/NeriPlayer-Desktop](https://github.com/cwuom/NeriPlayer-Desktop) | 多源在线播放音乐客户端（Tauri 2 + Vue 3 + Rust 音频引擎，内置 FFmpeg 解码库）。跟踪最新 Release，下载官方 amd64 deb 重打包；版本包含单调递增的构建 epoch，SHA256 从 `SHA256SUMS.txt` 校验 |
| dux-files | [duxlabs-dev/dux-files](https://github.com/duxlabs-dev/dux-files) | Dux Labs 的 Wayland 文件管理器（Quickshell UI + Go daemon，UI 经 go:embed 内嵌）。取官方预编译 linux-amd64 二进制重打包；运行时依赖官方仓库的 `quickshell` |
