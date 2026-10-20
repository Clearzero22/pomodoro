# TODO · 番茄钟新功能路线图

> 单一入口：本文件是总览，细节拆在 `todos/` 目录。每个任务一个文件，含需求/设计/验收标准，改完勾 `status` 即可。

## 看板

| 优先级 | 编号 | 任务 | 状态 | 预估 |
|--------|------|------|------|------|
| P0 | [001](./todos/001-audio-notification.md) | 计时结束提示音 + 系统通知 | ⬜ 待开始 | 1天 |
| P0 | [002](./todos/002-duration-settings.md) | 时长设置界面 (专注/休息可配置) | ⬜ 待开始 | 1-2天 |
| P0 | [003](./todos/003-persistence.md) | 跨会话持久化 (completed/配置落盘) | ⬜ 待开始 | 0.5天 |
| P1 | [004](./todos/004-tray-mini-window.md) | 托盘 + 迷你窗 + 全局快捷键 | ⬜ 待开始 | 2天 |
| P1 | [005](./todos/005-statistics.md) | 今日/本周统计 + 历史记录 | ⬜ 待开始 | 1.5天 |
| P1 | [006](./todos/006-themes-keybindings.md) | 主题切换 + 快捷键自定义 | ⬜ 待开始 | 1天 |
| P2 | [007](./todos/007-desktop-packaging.md) | 桌面安装包完善 (Win/macOS/Linux) | ⬜ 待开始 | 1天 |
| P2 | [008](./todos/008-mobile-enhance.md) | 移动端增强 (安全区/状态栏/离线) | ⬜ 待开始 | 1天 |
| P0 | [009](./todos/009-cloud-crud.md) | **联网数据 CRUD + 云端同步** | ⬜ 待开始 | 3天 |

## 里程碑

- **M1 可用增强 (P0)**: 001 + 002 + 003 → 日常可用的完整番茄钟
- **M2 桌面体验 (P1)**: 004 + 005 + 006 → 桌面端达到产品级
- **M3 发布 (P2)**: 007 + 008 → 可分发到各平台
- **M4 云端 (P0)**: 009 → 四端数据互通 + 离线同步 (含完整测试方案)

## 约定

- 分支命名：`feat/001-audio-notification`
- 提交前缀：`feat(001): ...` / `fix(003): ...`
- 每个任务完成需：`cargo test -p pomodoro` 通过 + `make desktop` + `make web-release` 手动验证
- 涉及 `crates/pomodoro` 的改动必须保持 `#[cfg]` 为零，平台差异留在两个入口 crate

## 快速开始

```sh
# 领一个任务
cat todos/001-audio-notification.md

# 开发
make desktop        # 桌面热跑
make web            # 浏览器热跑 (改 Rust 后重跑 ./scripts/build-wasm.sh)

# 验收
cargo test -p pomodoro
```
