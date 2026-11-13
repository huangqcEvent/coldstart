//! 桌面命令错误序列化。
//!
//! 把应用层错误转换为稳定的 `{ code, message }` 结构，供前端统一展示和恢复；
//! 序列化前不得附加内部堆栈、文件内容或凭据。

#[derive(Debug, serde::Serialize)]
pub struct CommandError {
    pub code: String,
    pub message: String,
}

impl From<coldstart_app::AppError> for CommandError {
    fn from(error: coldstart_app::AppError) -> Self {
        Self {
            code: error.code().to_owned(),
            message: error.to_string(),
        }
    }
}
