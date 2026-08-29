# arch-cloud-pkgs

个人 Arch Linux 包云端构建仓库：用 GitHub Actions 在 `archlinux` 容器里跑 `makepkg`，本机零编译零依赖污染。

## 已收录包

| 包名 | 上游 | 说明 |
|------|------|------|
| [we-layerd](https://github.com/Aromatic05/we-layerd) | v0.2.7 | 原生 Wallpaper Engine 运行时（scene/video/web，支持 niri）|
| [xplorer](https://github.com/kimlimjustin/xplorer) | v1.0.0-alpha.1 | 现代文件管理器（Tauri 2 + React，AI/Git/终端集成，从上游 deb 重打包）|

## 使用方法

### 触发构建
1. 推送代码后自动触发；或到 **Actions → Build packages → Run workflow** 手动触发
2. 等待约 15~30 分钟（CEF + Rust 编译较慢）
3. 到该次运行页面底部下载 artifact `arch-packages`

### 本地安装
```bash
sudo pacman -U *.pacman
```

### 升级某个包
编辑对应 `pkgs/<包名>/PKGBUILD` 的 `pkgver`，commit & push 即可自动重新构建。

## 添加新包

```bash
mkdir pkgs/<新包名>
# 参照 pkgs/we-layerd/PKGBUILD 编写
git add && git commit && git push   # 自动触发构建
```

## 注意事项

- 构建在 `archlinux:base-devel` 官方容器中进行，glibc 与本机滚动版本一致
- makepkg 以普通用户 `builder`（带 NOPASSWD sudo）运行，可自动解析 makedepends
- 未启用 cargo 缓存：每次全量编译，如需提速后续可加 actions/cache

## Roadmap

- [ ] 二期：nvchecker 自动检测上游发版并触发构建
- [ ] 三期：Releases 当私人 pacman 仓库（repo-add），实现 pacman -Syu 直接更新
