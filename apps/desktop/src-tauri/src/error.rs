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
