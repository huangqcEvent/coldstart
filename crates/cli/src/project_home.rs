use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use coldstart_core::{Project, ProjectManifest};
use coldstart_storage::Database;
use tempfile::NamedTempFile;

use crate::CliError;

const MANIFEST_FILE: &str = "coldstart.project.json";
const STATE_DIRECTORY: &str = ".coldstart";
const DATABASE_FILE: &str = "state.db";
const PROJECT_DIRECTORIES: [&str; 8] = [
    "docs/requirements",
    "docs/architecture",
    "docs/decisions",
    "docs/release",
    ".coldstart/artifacts",
    ".coldstart/snapshots",
    ".coldstart",
    "docs",
];

pub struct ProjectHome {
    root: PathBuf,
}

impl ProjectHome {
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn initialize(&self, name: &str) -> Result<Project, CliError> {
        if self.root.exists() && !self.root.is_dir() {
            return Err(CliError::NotADirectory(self.root.clone()));
        }

        let project = Project::new(name, std::path::absolute(&self.root)?)?;
        let manifest_path = self.manifest_path();
        if manifest_path.exists() {
            return Err(CliError::AlreadyInitialized(self.root.clone()));
        }

        let database_path = self.database_path();
        if database_path.exists() {
            return Err(CliError::StateAlreadyExists(database_path));
        }

        fs::create_dir_all(&self.root)?;
        let manifest = ProjectManifest::from_project(&project);
        let mut encoded = serde_json::to_vec_pretty(&manifest)?;
        encoded.push(b'\n');
        let temporary_manifest = write_manifest_temp(&self.root, &encoded)?;
        publish_manifest(temporary_manifest, &self.root, &manifest_path)?;

        let mut cleanup = CleanupSet::default();
        cleanup.own(manifest_path);
        for path in database_files(&database_path) {
            cleanup.track_if_absent(path);
        }

        let result: Result<(), CliError> = (|| {
            for relative in PROJECT_DIRECTORIES {
                fs::create_dir_all(self.root.join(relative))?;
            }

            let mut database = Database::open(&database_path)?;
            database.initialize_project(&project)?;
            Ok(())
        })();

        match result {
            Ok(()) => Ok(project),
            Err(source) => Err(rollback(source, cleanup)),
        }
    }

    fn manifest_path(&self) -> PathBuf {
        self.root.join(MANIFEST_FILE)
    }

    fn database_path(&self) -> PathBuf {
        self.root.join(STATE_DIRECTORY).join(DATABASE_FILE)
    }
}

fn write_manifest_temp(root: &Path, encoded: &[u8]) -> Result<NamedTempFile, CliError> {
    let mut temporary = NamedTempFile::new_in(root)?;
    temporary.write_all(encoded)?;
    temporary.as_file().sync_all()?;
    Ok(temporary)
}

fn publish_manifest(
    temporary: NamedTempFile,
    root: &Path,
    manifest_path: &Path,
) -> Result<(), CliError> {
    match temporary.persist_noclobber(manifest_path) {
        Ok(_) => Ok(()),
        Err(error)
            if error.error.kind() == io::ErrorKind::AlreadyExists || manifest_path.exists() =>
        {
            Err(CliError::AlreadyInitialized(root.to_path_buf()))
        }
        Err(error) => Err(CliError::Io(error.error)),
    }
}

#[derive(Debug, Default)]
struct CleanupSet {
    candidates: Vec<CleanupCandidate>,
}

#[derive(Debug)]
struct CleanupCandidate {
    path: PathBuf,
    owned: bool,
}

impl CleanupSet {
    fn own(&mut self, path: impl Into<PathBuf>) {
        self.candidates.push(CleanupCandidate {
            path: path.into(),
            owned: true,
        });
    }

    fn track_if_absent(&mut self, path: impl Into<PathBuf>) {
        let path = path.into();
        let owned = !path.exists();
        self.candidates.push(CleanupCandidate { path, owned });
    }

    fn remove_owned(&mut self) -> Vec<PathBuf> {
        let mut residual_paths = Vec::new();

        for candidate in self.candidates.drain(..) {
            if !candidate.owned {
                continue;
            }

            match fs::remove_file(&candidate.path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(_) => residual_paths.push(candidate.path),
            }
        }

        residual_paths
    }
}

fn rollback(source: CliError, mut cleanup: CleanupSet) -> CliError {
    let residual_paths = cleanup.remove_owned();
    if residual_paths.is_empty() {
        source
    } else {
        CliError::RollbackFailed {
            source: Box::new(source),
            residual_paths,
        }
    }
}

fn database_files(database_path: &Path) -> [PathBuf; 3] {
    [
        database_path.to_path_buf(),
        sidecar_path(database_path, "-wal"),
        sidecar_path(database_path, "-shm"),
    ]
}

fn sidecar_path(database_path: &Path, suffix: &str) -> PathBuf {
    let mut candidate: OsString = database_path.as_os_str().to_owned();
    candidate.push(suffix);
    PathBuf::from(candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn manifest_publication_does_not_overwrite_a_competing_writer() {
        let temp = TempDir::new().expect("temp directory must exist");
        let manifest_path = temp.path().join(MANIFEST_FILE);
        let temporary = write_manifest_temp(temp.path(), b"complete manifest\n")
            .expect("temporary manifest must be written");
        fs::write(&manifest_path, b"competing manifest\n").expect("competing manifest must exist");

        let error = publish_manifest(temporary, temp.path(), &manifest_path)
            .expect_err("publication must not overwrite");

        assert!(matches!(error, CliError::AlreadyInitialized(_)));
        assert_eq!(
            fs::read(&manifest_path).expect("manifest must remain readable"),
            b"competing manifest\n"
        );
        assert_eq!(
            fs::read_dir(temp.path())
                .expect("temp directory must remain readable")
                .count(),
            1,
            "failed publication left a temporary file"
        );
    }

    #[test]
    fn cleanup_removes_only_files_owned_by_this_initialization() {
        let temp = TempDir::new().expect("temp directory must exist");
        let owned_path = temp.path().join("owned.db");
        let preexisting_path = temp.path().join("preexisting.db-wal");
        let mut cleanup = CleanupSet::default();

        cleanup.track_if_absent(&owned_path);
        fs::write(&owned_path, b"owned").expect("owned file must be written");
        fs::write(&preexisting_path, b"keep").expect("preexisting file must be written");
        cleanup.track_if_absent(&preexisting_path);

        let residual_paths = cleanup.remove_owned();

        assert!(residual_paths.is_empty());
        assert!(!owned_path.exists());
        assert_eq!(
            fs::read(&preexisting_path).expect("preexisting file must remain readable"),
            b"keep"
        );
    }

    #[test]
    fn rollback_reports_cleanup_failures_as_residual_paths() {
        let temp = TempDir::new().expect("temp directory must exist");
        let residual_path = temp.path().join("cannot-remove-with-remove-file");
        fs::create_dir(&residual_path).expect("residual directory must be created");
        let mut cleanup = CleanupSet::default();
        cleanup.own(&residual_path);

        let error = rollback(CliError::NotADirectory(temp.path().to_path_buf()), cleanup);

        let CliError::RollbackFailed {
            source,
            residual_paths,
        } = error
        else {
            panic!("cleanup failure must produce RollbackFailed");
        };
        assert!(matches!(*source, CliError::NotADirectory(_)));
        assert_eq!(residual_paths, vec![residual_path]);
    }
}
