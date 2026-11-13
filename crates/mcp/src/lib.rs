//! ColdStart MCP 服务入口。
//!
//! MCP 通过 stdio 被动响应 Codex 调用，不开放本地端口；所有工具都只作用于
//! `--project-home` 指定的单一项目，并遵守主动领取与最小上下文原则。

mod error;
mod server;
mod types;

pub use server::ColdStartMcp;
