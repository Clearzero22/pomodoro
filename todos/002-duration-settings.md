# 002 · 时长设置界面

- **状态**: ⬜ 待开始
- **优先级**: P0
- **分支**: `feat/002-duration-settings`
- **依赖**: 003 (持久化) 可并行，先内存版

## 目标

用户可自定义专注/休息时长，并立即生效。

## 需求

1. 设置面板：专注 15/25/30/45/60 分钟，休息 5/10/15 分钟（步进 1 分钟，范围 1-60）
2. 切换时长后：若当前阶段未运行，剩余时间立即更新；若运行中，下一次该阶段生效（避免中途跳变）
3. 提供「恢复默认」按钮 (25/5)
4. 快捷键：`Cmd+,` 打开设置

## 设计要点

- 状态放 `Pomodoro` 结构体：`focus_duration: Duration` / `break_duration: Duration`，默认 `FOCUS_SECONDS/BREAK_SECONDS`
- `Phase::duration()` 改为实例方法 `phase.duration_with(config)` 或 `self.config.duration_for(phase)`
- UI 用 `gpui_kit` 的 `Input` + `Slider` + `Button`，设置面板作为 `div` 覆盖层或新视图
- 配置变更发射 `TimerEvent::ConfigChanged`（可选）供持久化监听

## 任务拆解

- [ ] 1. `PomodoroConfig` 结构体 + 默认值
- [ ] 2. `Phase::duration` 重构为可配置
- [ ] 3. 设置面板 UI + 打开/关闭交互
- [ ] 4. 运行时生效逻辑 + 边界测试
- [ ] 5. 与 003 联调持久化

## 验收

- [ ] 修改专注为 15 分钟后，重置显示 15:00
- [ ] 运行中修改不影响当前倒计时，下一轮生效
- [ ] `cargo test -p pomodoro` 新增设置相关用例通过

## 界面草图

```
[专注 25m ▼] [休息 5m ▼]  [恢复默认]
  ●──────────○ 15  25  45  60
```
