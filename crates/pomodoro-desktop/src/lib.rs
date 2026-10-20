//! 桌面端入口的库形式。
//!
//! 可执行入口在 `main.rs`；这里把 `api` 与 `persistence` 暴露成库，
//! 好让集成测试能直接对着真的后端跑 `HttpApi`。

pub mod api;
pub mod persistence;
