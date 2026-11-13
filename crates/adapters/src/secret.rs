//! Windows Credential Manager 凭据适配器。
//!
//! 凭据按项目与环境命名，读写只在 Rust 内存中短暂存在；对外只暴露是否存在和
//! 引用标识，不回显 Secret，也不把凭据写入日志或项目目录。

use std::{collections::BTreeMap, fmt, sync::Mutex};

use thiserror::Error;

const KEYRING_SERVICE: &str = "ColdStart";

pub trait SecretStore: Send + Sync {
    fn set(&self, secret_ref: &str, secret: &str) -> Result<(), SecretStoreError>;
    fn get(&self, secret_ref: &str) -> Result<String, SecretStoreError>;
    fn delete(&self, secret_ref: &str) -> Result<(), SecretStoreError>;
}

#[derive(Debug, Error)]
pub enum SecretStoreError {
    #[error("secret reference cannot be empty")]
    EmptyReference,
    #[error("secret value cannot be empty")]
    EmptySecret,
    #[error("secret not found: {0}")]
    NotFound(String),
    #[error("system credential store failed: {0}")]
    Backend(String),
}

#[derive(Default)]
pub struct MemorySecretStore {
    entries: Mutex<BTreeMap<String, String>>,
}

impl fmt::Debug for MemorySecretStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self
            .entries
            .lock()
            .map(|entries| entries.len())
            .unwrap_or_default();
        formatter
            .debug_struct("MemorySecretStore")
            .field("entries", &format!("[REDACTED] ({count} entries)"))
            .finish()
    }
}

impl SecretStore for MemorySecretStore {
    fn set(&self, secret_ref: &str, secret: &str) -> Result<(), SecretStoreError> {
        validate_secret_ref(secret_ref)?;
        if secret.is_empty() {
            return Err(SecretStoreError::EmptySecret);
        }
        let mut entries = self.entries.lock().map_err(|_| {
            SecretStoreError::Backend("credential store lock was poisoned".to_owned())
        })?;
        entries.insert(secret_ref.to_owned(), secret.to_owned());
        Ok(())
    }

    fn get(&self, secret_ref: &str) -> Result<String, SecretStoreError> {
        validate_secret_ref(secret_ref)?;
        let entries = self.entries.lock().map_err(|_| {
            SecretStoreError::Backend("credential store lock was poisoned".to_owned())
        })?;
        entries
            .get(secret_ref)
            .cloned()
            .ok_or_else(|| SecretStoreError::NotFound(secret_ref.to_owned()))
    }

    fn delete(&self, secret_ref: &str) -> Result<(), SecretStoreError> {
        validate_secret_ref(secret_ref)?;
        let mut entries = self.entries.lock().map_err(|_| {
            SecretStoreError::Backend("credential store lock was poisoned".to_owned())
        })?;
        entries.remove(secret_ref);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WindowsSecretStore;

impl WindowsSecretStore {
    pub fn new() -> Self {
        Self
    }
}

impl SecretStore for WindowsSecretStore {
    fn set(&self, secret_ref: &str, secret: &str) -> Result<(), SecretStoreError> {
        validate_secret_ref(secret_ref)?;
        if secret.is_empty() {
            return Err(SecretStoreError::EmptySecret);
        }
        entry(secret_ref)?
            .set_password(secret)
            .map_err(backend_error)
    }

    fn get(&self, secret_ref: &str) -> Result<String, SecretStoreError> {
        validate_secret_ref(secret_ref)?;
        match entry(secret_ref)?.get_password() {
            Ok(secret) => Ok(secret),
            Err(keyring::Error::NoEntry) => Err(SecretStoreError::NotFound(secret_ref.to_owned())),
            Err(error) => Err(backend_error(error)),
        }
    }

    fn delete(&self, secret_ref: &str) -> Result<(), SecretStoreError> {
        validate_secret_ref(secret_ref)?;
        match entry(secret_ref)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(backend_error(error)),
        }
    }
}

fn validate_secret_ref(secret_ref: &str) -> Result<(), SecretStoreError> {
    if secret_ref.trim().is_empty() {
        Err(SecretStoreError::EmptyReference)
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn entry(secret_ref: &str) -> Result<keyring::Entry, SecretStoreError> {
    keyring::Entry::new(KEYRING_SERVICE, secret_ref).map_err(backend_error)
}

#[cfg(not(windows))]
fn entry(_secret_ref: &str) -> Result<keyring::Entry, SecretStoreError> {
    Err(SecretStoreError::Backend(
        "Windows Credential Manager is only available on Windows".to_owned(),
    ))
}

fn backend_error(error: keyring::Error) -> SecretStoreError {
    SecretStoreError::Backend(error.to_string())
}
