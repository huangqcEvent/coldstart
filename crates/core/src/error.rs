use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProjectError {
    #[error("project name cannot be empty")]
    EmptyName,
    #[error("project name must be at most 80 characters")]
    NameTooLong,
}
