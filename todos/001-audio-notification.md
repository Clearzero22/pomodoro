# 001 · 计时结束提示音 + 系统通知

- **状态**: ⬜ 待开始
- **优先级**: P0
- **分支**: `feat/001-audio-notification`
- **依赖**: 无

## 目标

专注/休息结束时有可感知的提醒，桌面端和浏览器端行为一致，后台也能收到。

## 需求

1. 桌面：阶段结束播放提示音（wav/ogg 内嵌），可选静音
2. 桌面：系统通知（Windows Toast / macOS Notification / Linux）
3. Web/PWA：复用已有的 `TimerEvent` → `__pomodoroNotify` 链路，补齐 `Web Notifications` 权限请求 + 音频播放
4. 设置里可开关声音/通知（为 002 预留接口，先用默认值 true）

## 设计要点

- 音频资源放 `crates/pomodoro-web/www/public/sounds/` + 桌面端 `gpui_kit::assets` 内嵌，避免运行时下载
- 桌面端用 `rodio` 或 `kira`（选轻量的），Web 端用 `Audio` 元素
- 复用 `crates/pomodoro/src/lib.rs` 的 `TimerEvent::PhaseCompleted`，在 `pomodoro-web/src/lib.rs:install_notifications` 旁边加 `install_audio`
- 浏览器自动播放限制：首次用户手势后解锁 AudioContext

## 任务拆解

- [ ] 1. 选型 + 添加依赖 (`rodio` vs `kira`，Web 端无需依赖)
- [ ] 2. 桌面端播放通路 + 单元测试
- [ ] 3. Web 端 `Audio` 播放 + 权限请求 (`notifications.js` 扩展)
- [ ] 4. 静音开关 (内存状态，持久化留给 003)
- [ ] 5. 真机验证：桌面后台、浏览器后台标签、PWA 独立窗口

## 验收

- [ ] `cargo test -p pomodoro` 通过
- [ ] 桌面 `make desktop` 结束时有声音 + 系统通知
- [ ] 浏览器 `make web` 后台标签也能收到通知（Chrome/Firefox）
- [ ] 静音后不再播放

## 备注

参考 `docs/mobile.md` 的通知预约思路：开始时就 `schedule`，暂停/重置时 `cancel`
