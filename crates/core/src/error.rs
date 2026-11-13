//! 领域错误及其稳定错误码。
//!
//! 错误码是对外协议的一部分，UI、MCP 和日志依赖它做可恢复判断；错误消息只描述
//! 可安全展示的原因，不包含 Secret、租约令牌或完整请求内容。

use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProjectError {
    #[error("project name cannot be empty")]
    EmptyName,
    #[error("project name must be at most 80 characters")]
    NameTooLong,
}
