# clavis-shell 本地补丁说明

CI 构建 `clavis-shell` 时克隆上游 [StatIndet/quickshell](https://github.com/StatIndet/quickshell) 的 `main`
当前源码，然后按文件名顺序应用本目录下全部 `*.patch`（见
`.github/scripts/prepare-upstream-source.sh`）。补丁会被烘焙进源码包与安装包，
桌面机运行期无需任何手工改动。

每个补丁的性质与移除条件见下文：0001/0002 是上游尚未修复时的临时 workaround，
0003 是本仓库选择的播放器歌词模式，不应在上游仍保留旧获取链路时直接删除。
上游合入等价实现后，本目录对应补丁可被删除。删除前可用 CI 的自动跳过逻辑确认：`prepare-upstream-source.sh`
会对每个补丁先 `git apply --check`（能应用就应用），失败再试
`git apply --reverse --check`（说明上游已内置该修复，跳过并提示）；两者都失败才会
报错。因此**上游修复后直接删掉对应 `.patch` 文件即可，无需改构建脚本**。

---

## 0001-bar-wheel-scroll-and-brightness-init.patch

**改动文件**：`Modules/Bar/QuickSettings/Brightness.qml`、`Modules/Bar/QuickSettings/Microphone.qml`

**问题**：Qt 6.12 上 `RippleButton` 内部的 `MouseArea` 会先消费滚轮事件，
导致这两个控件的 `WheelHandler` 收不到 `onWheel`，任务栏亮度/麦克风音量滚轮调节无效。

**Workaround**：把 `WheelHandler` 换成 `MouseArea`（`acceptedButtons: Qt.NoButton`
保持点击行为不变），以 `Qt.NoButton` 鼠标区域直接接收滚轮事件。

**上游状态**：无对应 issue/PR；属 Qt 6.12 事件分发行为变化。

**移除条件**：上游在 Qt 6.12 环境修复滚轮调节，或改用其他不冲突的事件处理方式。
`git apply --reverse --check` 通过即说明已内置，可删除。

---

## 0002-idle-monitor-rebuild-workaround.patch

**改动文件**：`Services/IdleService.qml`

**问题**：quickshell 的 `IdleMonitor` 存在上游 bug
[#938](https://github.com/quickshell-mirror/quickshell/issues/938)
（修复 PR [#1097](https://github.com/quickshell-mirror/quickshell/pull/1097)
截至 2026-10 未合并，Arch extra 仓库仍为 0.3.2）：

`IdleMonitor.timeout` / `respectInhibitors` 运行时变化时，quickshell 销毁旧
`ext_idle_notification_v1` 并新建一个。若新对象恰好分配到**同一堆地址**，
`bNotification`（指针绑定属性）比较相等，`isIdle` 的绑定表达式不会重新求值——
即使合成器照常发送 `idled` 事件（Rust 层日志正常打 `marked idle`），QML 层的
`isIdle` 永远保持 `false`，锁屏/息屏/休眠全部静默失效。

**Clavis 侧触发机制**：`IdleService` 的策略通过 `FileView` **异步**加载
`~/.config/clavis/idle-policy.json`。四个 `IdleMonitor`（dim/lock/displayOff/suspend）
在组件创建时先按默认值（600s/900s 等）创建通知，`loadPolicy` 落地用户配置
（如 300s/600s/7200s）后属性变化 → 同地址销毁重建 → 全部失联。
症状：配置了自动锁屏/休眠但永不触发；手动锁屏（快捷键或
`quickshell ipc -c clavis call lock open`）正常。

**Workaround**（两部分，缺一不可）：

1. `_monitorEnabled()` 增加 `root.policyReady` 前置条件：策略加载完成前不创建任何
   idle 通知，保证每个 stage 只用最终配置创建一次通知（启动场景）。
2. 新增 `scheduleRecycle(monitor)`：四个 `IdleMonitor` 的
   `onTimeoutChanged` / `onRespectInhibitorsChanged` 触发时先 `enabled=false` 再
   `Qt.callLater` 恢复 `true`，强制通知销毁重建（`bNotification` 经过 `null`
   重新订阅），覆盖设置面板里运行时调整场景。

**移除条件**：quickshell 合并 #1097（或等价修复）且 Arch extra 仓库升级到包含该
修复的版本后，删除本补丁；同时建议在桌面机卸载旧包、安装新构建包
（`sudo pacman -U` 新 artifact），并重启 `clavis-shell` 验证自动锁屏恢复。

---

## 0003-mpris-astext-lyrics.patch

**基线**：上游 main `fc1d5fa70498a05334e185d923bcf79f156799a6`。

**行为**：`Clavis.Lyrics` 只接收当前 MPRIS 播放器的 `Metadata["xesam:asText"]` 字符串。
删除本地 LRC 文件读取、Shell 歌词缓存读写、LRCLIB/网易云联网搜索与歌词请求、候选选择和 LRC 时间轴。
不写入或删除用户已存在的缓存/歌词文件，不创建歌词中转文件。原生歌词模块不再依赖 Qt Network。
播放器不提供该字段时显示无歌词；字段移除、播放器退出/切换时清除原文本。
监听 MPRIS 元数据更新，因此同一首歌后到达的字幕也能替换显示，不按歌名重查。

**显示范围**：只显示原样纯文本，水平歌词区可手动滚动，竖向紧凑区显示第一行；
不按播放位置滚动、不解析时间戳、不做逐字高亮，文本不会被解释成富文本。
过大（超过 512 Ki UTF-16 单元）、非字符串、无效 UTF-16 或含 NUL 的数据被拒绝。

**验证**：替换已删除 provider/LRC 行为的旧测试，以独立的 C++ 元数据消费测试覆盖
纯文本、多种换行、字段移除/播放器切换以及非法数据清理。通过云端原有 CTest 执行，
不依赖 Cirrus 仓库、真实网络、用户缓存或桌面会话。QML 仅格式化改动文件。

**移除条件**：上游支持相同的标准字段消费且可以彻底关闭旧歌词文件/缓存/联网链路后，
先确认行为等价再移除；不能仅因上游有新的联网歌词功能而删除。
