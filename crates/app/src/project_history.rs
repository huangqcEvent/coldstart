//! 应用级项目访问历史。
//!
//! 该存储只记录最近打开的项目路径、名称和时间，位于 Tauri 应用数据目录，而不是
//! 任一项目目录。它不保存 Secret、租约或项目业务状态；清空历史只删除索引，
//! 永远不会删除项目清单、SQLite 数据库或业务仓库。

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tempfile::NamedTempFile;

use crate::{AppError, ProjectService};

const HISTORY_FILE_NAME: &str = "project-history.json";
const PROJECT_HISTORY_SCHEMA_VERSION: u32 = 1;
const MAX_RECENT_PROJECTS: usize = 12;

/// 保存在应用数据目录中的单个最近项目索引。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredRecentProject {
    root_path: String,
    name: String,
    last_opened_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct StoredProjectHistory {
    schema_version: u32,
    last_project_path: Option<String>,
    projects: Vec<StoredRecentProject>,
}

impl Default for StoredProjectHistory {
    fn default() -> Self {
        Self {
            schema_version: PROJECT_HISTORY_SCHEMA_VERSION,
            last_project_path: None,
            projects: Vec::new(),
        }
    }
}

/// 返回给桌面的最近项目条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentProjectView {
    pub root_path: String,
    pub name: String,
    pub last_opened_at: DateTime<Utc>,
    pub available: bool,
}

/// 返回给桌面的应用级项目历史视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectHistoryView {
    pub schema_version: u32,
    pub last_project_path: Option<String>,
    pub projects: Vec<RecentProjectView>,
}

struct ProjectHistoryStoreInner {
    config_dir: PathBuf,
    lock: Mutex<()>,
}

/// 线程安全的项目历史存储。
///
/// 所有读改写操作都在同一进程内串行化，并通过同目录临时文件原子替换配置；
/// 使用 `Clone` 是为了把阻塞的文件与项目校验工作移出 Tauri 主线程。
#[derive(Clone)]
pub struct ProjectHistoryStore {
    inner: Arc<ProjectHistoryStoreInner>,
}

impl ProjectHistoryStore {
    /// 使用应用数据目录创建存储；目录在首次写入时才创建。
    pub fn new(config_dir: impl Into<PathBuf>) -> Self {
        Self {
            inner: Arc::new(ProjectHistoryStoreInner {
                config_dir: config_dir.into(),
                lock: Mutex::new(()),
            }),
        }
    }

    /// 读取最近项目并逐个校验项目是否仍可打开。
    ///
    /// 失效路径会保留在历史中并标记为不可用，便于用户确认后手动移除。
    pub fn read(&self) -> Result<ProjectHistoryView, AppError> {
        let _guard = self.lock()?;
        Ok(self.view_unlocked())
    }

    /// 校验项目目录后将其置于最近列表首位，并记录为上次打开项目。
    pub fn remember(&self, project_home: &Path) -> Result<ProjectHistoryView, AppError> {
        let _guard = self.lock()?;
        let requested = dunce::canonicalize(project_home)?;
        let status = ProjectService::new().read_status(requested)?;
        let root_path = status
            .root_path
            .to_str()
            .ok_or_else(|| AppError::NonUtf8Path(status.root_path.clone()))?
            .to_owned();

        let mut stored = self.read_stored_unlocked();
        stored
            .projects
            .retain(|project| !same_path(&project.root_path, &root_path));
        stored.projects.insert(
            0,
            StoredRecentProject {
                root_path: root_path.clone(),
                name: status.name,
                last_opened_at: Utc::now(),
            },
        );
        stored.projects.truncate(MAX_RECENT_PROJECTS);
        stored.last_project_path = Some(root_path);
        stored.schema_version = PROJECT_HISTORY_SCHEMA_VERSION;
        self.write_unlocked(&stored)?;
        Ok(self.view_from_stored(stored))
    }

    /// 从最近列表中移除指定路径；若它是上次项目，则退回到剩余列表首项。
    pub fn forget(&self, project_home: &Path) -> Result<ProjectHistoryView, AppError> {
        let _guard = self.lock()?;
        let path = normalized_path_string(project_home);
        let mut stored = self.read_stored_unlocked();
        stored
            .projects
            .retain(|project| !same_path(&project.root_path, &path));
        if stored
            .last_project_path
            .as_deref()
            .is_some_and(|last_path| same_path(last_path, &path))
        {
            stored.last_project_path = stored
                .projects
                .first()
                .map(|project| project.root_path.clone());
        }
        self.write_unlocked(&stored)?;
        Ok(self.view_from_stored(stored))
    }

    /// 清空应用级访问历史，不触碰任何项目文件。
    pub fn clear(&self) -> Result<ProjectHistoryView, AppError> {
        let _guard = self.lock()?;
        let stored = StoredProjectHistory::default();
        self.write_unlocked(&stored)?;
        Ok(self.view_from_stored(stored))
    }

    fn lock(&self) -> Result<MutexGuard<'_, ()>, AppError> {
        self.inner
            .lock
            .lock()
            .map_err(|_| AppError::Io(io::Error::other("project history lock is poisoned")))
    }

    fn config_path(&self) -> PathBuf {
        self.inner.config_dir.join(HISTORY_FILE_NAME)
    }

    fn read_stored_unlocked(&self) -> StoredProjectHistory {
        let bytes = match fs::read(self.config_path()) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return StoredProjectHistory::default();
            }
            Err(_) => return StoredProjectHistory::default(),
        };
        let stored: StoredProjectHistory = match serde_json::from_slice(&bytes) {
            Ok(stored) => stored,
            Err(_) => return StoredProjectHistory::default(),
        };
        if stored.schema_version != PROJECT_HISTORY_SCHEMA_VERSION {
            return StoredProjectHistory::default();
        }
        stored
    }

    fn write_unlocked(&self, stored: &StoredProjectHistory) -> Result<(), AppError> {
        fs::create_dir_all(&self.inner.config_dir)?;
        let mut encoded = serde_json::to_vec_pretty(stored)?;
        encoded.push(b'\n');
        let mut temporary = NamedTempFile::new_in(&self.inner.config_dir)?;
        temporary.write_all(&encoded)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(self.config_path())
            .map_err(|error| AppError::Io(error.error))?;
        Ok(())
    }

    fn view_unlocked(&self) -> ProjectHistoryView {
        self.view_from_stored(self.read_stored_unlocked())
    }

    fn view_from_stored(&self, stored: StoredProjectHistory) -> ProjectHistoryView {
        let projects = stored
            .projects
            .into_iter()
            .map(|project| {
                let available = ProjectService::new()
                    .read_status(PathBuf::from(&project.root_path))
                    .is_ok();
                RecentProjectView {
                    root_path: project.root_path,
                    name: project.name,
                    last_opened_at: project.last_opened_at,
                    available,
                }
            })
            .collect::<Vec<_>>();
        let last_project_path = stored.last_project_path.filter(|last_path| {
            projects
                .iter()
                .any(|project| same_path(&project.root_path, last_path))
        });
        ProjectHistoryView {
            schema_version: PROJECT_HISTORY_SCHEMA_VERSION,
            last_project_path,
            projects,
        }
    }
}

fn normalized_path_string(path: &Path) -> String {
    dunce::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

fn same_path(left: &str, right: &str) -> bool {
    if cfg!(windows) {
        left.eq_ignore_ascii_case(right)
    } else {
        left == right
    }
}
