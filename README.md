# arch-cloud-pkgs

个人 Arch Linux 包云端构建仓库：用 GitHub Actions 在 `archlinux` 容器里跑 `makepkg`，本机零编译零依赖污染。

## 已收录包

| 包名 | 上游 | 说明 |
|------|------|------|
| [we-layerd](https://github.com/Aromatic05/we-layerd) | v0.2.7 | 原生 Wallpaper Engine 运行时（scene/video/web，支持 niri）|
| [spacedrive](https://github.com/spacedriveapp/spacedrive) | v2.0.0-alpha.2 (预发布) | 跨平台通用文件管理器（Tauri 2 + VDFS 虚拟分布式文件系统，从上游 deb 重打包）|
| [wayland-pipewire-idle-inhibit-aur](https://github.com/rafaelrc7/wayland-pipewire-idle-inhibit) | v0.7.1 | 播放声音时抑制 Wayland idle（包名带 `-aur` 后缀以避免产物匹配问题，`provides` 原包名）|

## 使用方法

### 触发构建
1. 推送代码后自动触发；或到 **Actions → Build packages → Run workflow** 手动触发
2. 等待构建完成（见下方"构建加速"）
3. 到该次运行页面底部下载 artifact `arch-packages`（合并产物）；若某个包构建失败，其余成功的包可从 `arch-packages-<包名>` 单独下载

### 本地安装
```bash
sudo pacman -U *.pacman
```

### 升级某个包
编辑对应 `pkgs/<包名>/PKGBUILD` 的 `pkgver`，commit & push 即可自动重新构建（缓存会因 PKGBUILD 变化自动失效，本次为全量重编）。

## 构建加速

- **只构建变更的包**：push 时按 diff 选出改动涉及的包，也可手动指定单个包
- **并行构建**：多个包同时变更时，每个包一个独立 job（matrix）并行执行，互不影响
- **构建缓存**：cargo registry + 构建目录（含 cargo target）按包缓存，同版本重建 = 增量编译（we-layerd 可从 15~30 分钟降到几分钟）；缓存受 GitHub 10GB/仓库 配额限制，旧版本缓存会自动淘汰
- **自动取消**：同一分支的新提交会取消仍在跑的旧构建，避免浪费 runner

## 添加新包

```bash
mkdir pkgs/<新包名>
# 参照 pkgs/we-layerd/PKGBUILD 编写
git add && git commit && git push   # 自动触发构建
```

## 注意事项

- 构建在 `archlinux:base-devel` 官方容器中进行，glibc 与本机滚动版本一致
- makepkg 以普通用户 `builder`（带 NOPASSWD sudo）运行，可自动解析 makedepends

## Roadmap

- [x] 一期：增量构建（只构建变更的包）+ 并行 matrix + cargo 缓存
- [ ] 二期：nvchecker 自动检测上游发版并触发构建
- [ ] 三期：Releases 当私人 pacman 仓库（repo-add），实现 pacman -Syu 直接更新
