# 003 · 跨会话持久化

- **状态**: ⬜ 待开始
- **优先级**: P0
- **分支**: `feat/003-persistence`
- **依赖**: 无

## 目标

关闭再打开后，已完成数和设置不丢失。

## 需求

1. 持久化字段：`completed`、`focus_duration`、`break_duration`、`sound_enabled`、`notifications_enabled` (为 001/002 预留)
2. 桌面端：`~/.config/pomodoro/config.json` 或 `gpui` 提供的路径
3. Web 端：`localStorage` (`pomodoro:config` / `pomodoro:completed`)
4. 启动时读取，变更时 500ms debounce 写入
5. 提供「清除数据」按钮

## 设计要点

- 平台差异隔离：`crates/pomodoro` 只暴露 `PomodoroState { completed, config }` 的 `serde` 结构，读写由入口 crate 注入 `Persistence` trait
- 桌面：`std::fs` + `serde_json`；Web：`web-sys::window().local_storage()`
- 避免在 `crates/pomodoro` 里出现 `#[cfg(target_family="wasm")]`
- 测试：用内存实现 `InMemoryPersistence` 测回放

## 任务拆解

- [ ] 1. `PomodoroState` + `PomodoroConfig` + `serde` 序列化
- [ ] 2. 定义 `Persistence` trait (`load`/`save`)
- [ ] 3. 桌面 `FilePersistence` 实现
- [ ] 4. Web `LocalStoragePersistence` 实现 + JS 桥接
- [ ] 5. 启动加载 + 变更自动保存 + 防抖
- [ ] 6. 清除数据 UI

## 验收

- [ ] 改设置后重启，配置保留
- [ ] 完成 3 个番茄后重启，计数仍为 3
- [ ] Web 端无痕模式/禁用 localStorage 时不崩溃（降级为内存）

## 数据格式

```json
{
  "completed": 12,
  "config": { "focus_minutes": 25, "break_minutes": 5, "sound": true }
}
```
