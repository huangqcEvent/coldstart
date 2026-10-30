use std::{
    collections::BTreeMap,
    fmt,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use chrono::Utc;
use coldstart_adapters::{
    CreatePrivateRepositoryRequest, DeviceFlowPoll, GitAdapter, GitHubClient, GitHubRepository,
    GitHubToken, GitRepositoryStatus, SecretStore, SecretStoreError, WindowsSecretStore,
    import_gh_token,
};
use coldstart_core::{
    CAPABILITY_ENVIRONMENTS, CapabilityState, CapabilityStatus, CredentialEnvironment,
    CredentialId, CredentialKind, CredentialMetadata, GitSyncRecord, ProjectId, Repository,
    RepositoryAuthMode, RepositorySyncPolicy, RepositoryVerificationCommand, TaskKind,
};
use coldstart_storage::StorageError;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{AppError, WorkItemView, context::OpenProject, plan_workspace::build_plan_workspace};

const GITHUB_PROVIDER: &str = "github";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryDraft {
    pub key: String,
    pub name: String,
    pub local_path: PathBuf,
    pub remote_url: String,
    pub default_branch: String,
    pub protected_branches: Vec<String>,
    pub auth_mode: RepositoryAuthMode,
    pub credential_id: Option<CredentialId>,
    pub sync_policy: RepositorySyncPolicy,
}

impl RepositoryDraft {
    fn into_repository(
        self,
        project_id: ProjectId,
    ) -> Result<Repository, coldstart_core::IntegrationValidationError> {
        Repository::new(
            project_id,
            self.key,
            self.name,
            self.local_path,
            self.remote_url,
            self.default_branch,
            self.protected_branches,
            self.auth_mode,
            self.credential_id,
            self.sync_policy,
        )
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveCredentialInput {
    pub name: String,
    pub provider: String,
    pub kind: CredentialKind,
    pub environment: CredentialEnvironment,
    pub secret: String,
}

impl fmt::Debug for SaveCredentialInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SaveCredentialInput")
            .field("name", &self.name)
            .field("provider", &self.provider)
            .field("kind", &self.kind)
            .field("environment", &self.environment)
            .field("secret", &"***")
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreatePrivateRepositoryInput {
    pub credential_id: CredentialId,
    pub repository_key: String,
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityStatusView {
    pub project_id: ProjectId,
    pub capability_id: coldstart_core::BlueprintEntityId,
    pub name: String,
    pub provider: String,
    pub environment: String,
    pub status: CapabilityStatus,
    pub credential_id: Option<CredentialId>,
    pub credential_ref: Option<String>,
    pub note: Option<String>,
    pub updated_at: chrono::DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseCenterView {
    pub prerequisites: Vec<WorkItemView>,
    pub milestones: Vec<WorkItemView>,
    pub approval_gates: Vec<WorkItemView>,
    pub human_tasks: Vec<WorkItemView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationWorkspace {
    pub project_id: ProjectId,
    pub repositories: Vec<Repository>,
    pub verification_commands: Vec<RepositoryVerificationView>,
    pub credentials: Vec<CredentialMetadata>,
    pub capabilities: Vec<CapabilityStatusView>,
    pub git_syncs: Vec<GitSyncRecord>,
    pub release: ReleaseCenterView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryVerificationView {
    pub repository_key: String,
    pub commands: Vec<RepositoryVerificationCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryValidationView {
    pub repository_key: String,
    pub remote_url: String,
    pub status: GitRepositoryStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitHubSessionView {
    pub credential_id: CredentialId,
    pub credential_ref: String,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceFlowView {
    pub session_id: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum GitHubDevicePollView {
    Pending { session_id: String, interval: u64 },
    SlowDown { session_id: String, interval: u64 },
    Authorized { session: GitHubSessionView },
    AccessDenied,
    Expired,
}

#[derive(Clone)]
struct PendingDeviceFlow {
    client_id: String,
    device_code: String,
}

pub struct IntegrationService {
    secrets: Arc<dyn SecretStore>,
    git: GitAdapter,
    github: Option<GitHubClient>,
    pending_device_flows: Mutex<BTreeMap<String, PendingDeviceFlow>>,
}

impl IntegrationService {
    pub fn new() -> Self {
        Self::with_dependencies(
            Arc::new(WindowsSecretStore::new()),
            GitAdapter::new(),
            GitHubClient::new().ok(),
        )
    }

    pub fn with_dependencies(
        secrets: Arc<dyn SecretStore>,
        git: GitAdapter,
        github: Option<GitHubClient>,
    ) -> Self {
        Self {
            secrets,
            git,
            github,
            pending_device_flows: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn load_workspace(&self, project_home: PathBuf) -> Result<IntegrationWorkspace, AppError> {
        let mut project = OpenProject::open(project_home)?;
        sync_capabilities_from_blueprint(&mut project)?;
        build_integration_workspace(&project)
    }

    pub fn validate_repository(
        &self,
        project_home: PathBuf,
        draft: &RepositoryDraft,
    ) -> Result<RepositoryValidationView, AppError> {
        let project = OpenProject::open(project_home)?;
        let repository = draft.clone().into_repository(project.project.id)?;
        let status = self.validate_git_repository(&repository)?;
        Ok(RepositoryValidationView {
            repository_key: repository.key,
            remote_url: repository.remote_url,
            status,
        })
    }

    pub fn save_repository(
        &self,
        project_home: PathBuf,
        draft: RepositoryDraft,
    ) -> Result<IntegrationWorkspace, AppError> {
        let mut project = OpenProject::open(project_home.clone())?;
        let repository = draft.into_repository(project.project.id)?;
        self.validate_git_repository(&repository)?;
        project.database.save_repository(&repository)?;
        self.load_workspace(project_home)
    }

    pub fn load_verification_commands(
        &self,
        project_home: PathBuf,
        repository_key: &str,
    ) -> Result<Vec<RepositoryVerificationCommand>, AppError> {
        let project = OpenProject::open(project_home)?;
        project
            .database
            .list_repository_verification_commands(&project.project.id, repository_key)
            .map_err(verification_storage_error)
    }

    pub fn save_verification_commands(
        &self,
        project_home: PathBuf,
        repository_key: &str,
        commands: Vec<RepositoryVerificationCommand>,
    ) -> Result<Vec<RepositoryVerificationCommand>, AppError> {
        let mut project = OpenProject::open(project_home)?;
        project
            .database
            .replace_repository_verification_commands(&project.project.id, repository_key, commands)
            .map_err(verification_storage_error)
    }

    pub fn save_credential(
        &self,
        project_home: PathBuf,
        input: SaveCredentialInput,
    ) -> Result<IntegrationWorkspace, AppError> {
        let mut project = OpenProject::open(project_home.clone())?;
        self.persist_credential(
            &mut project,
            input.name,
            input.provider,
            input.kind,
            input.environment,
            &input.secret,
        )?;
        self.load_workspace(project_home)
    }

    pub fn delete_credential(
        &self,
        project_home: PathBuf,
        credential_id: CredentialId,
    ) -> Result<IntegrationWorkspace, AppError> {
        let mut project = OpenProject::open(project_home.clone())?;
        let credential = project
            .database
            .credential(&project.project.id, &credential_id)?
            .ok_or(AppError::CredentialNotFound(credential_id))?;
        match self.secrets.delete(&credential.secret_ref) {
            Ok(()) | Err(SecretStoreError::NotFound(_)) => {}
            Err(error) => return Err(error.into()),
        }
        if !project
            .database
            .delete_credential(&project.project.id, &credential_id)?
        {
            return Err(AppError::CredentialNotFound(credential_id));
        }
        self.load_workspace(project_home)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_capability(
        &self,
        project_home: PathBuf,
        capability_id: coldstart_core::BlueprintEntityId,
        environment: String,
        status: CapabilityStatus,
        credential_id: Option<CredentialId>,
        note: Option<String>,
    ) -> Result<IntegrationWorkspace, AppError> {
        let mut project = OpenProject::open(project_home.clone())?;
        if let Some(credential_id) = credential_id
            && project
                .database
                .credential(&project.project.id, &credential_id)?
                .is_none()
        {
            return Err(AppError::CredentialNotFound(credential_id));
        }
        let existing = project
            .database
            .list_capability_states(&project.project.id)?
            .into_iter()
            .find(|state| state.capability_id == capability_id && state.environment == environment);
        let state = match existing {
            Some(mut state) => {
                state.status = status;
                state.credential_id = credential_id;
                state.note = note;
                state.updated_at = Utc::now();
                state.validate()?;
                state
            }
            None => {
                let blueprint = project
                    .database
                    .latest_blueprint(
                        &project.project.id,
                        coldstart_core::BlueprintStatus::Published,
                    )?
                    .ok_or(AppError::PublishedBlueprintRequired)?;
                let capability = blueprint
                    .content
                    .capabilities
                    .into_iter()
                    .find(|capability| capability.id == capability_id)
                    .ok_or_else(|| AppError::CapabilityNotFound {
                        capability_id: capability_id.to_string(),
                        environment: environment.clone(),
                    })?;
                CapabilityState::new(
                    coldstart_core::CapabilityDescriptor {
                        project_id: project.project.id,
                        capability_id,
                        name: capability.name,
                        provider: capability.provider,
                        environment,
                    },
                    status,
                    credential_id,
                    note,
                )?
            }
        };
        project.database.save_capability_state(&state)?;
        self.load_workspace(project_home)
    }

    pub fn get_capability_status(
        &self,
        project_home: PathBuf,
        capability_id: &coldstart_core::BlueprintEntityId,
        environment: &str,
    ) -> Result<CapabilityStatusView, AppError> {
        let project = OpenProject::open(project_home)?;
        let capabilities = load_capability_views(&project)?;
        capabilities
            .into_iter()
            .find(|state| state.capability_id == *capability_id && state.environment == environment)
            .ok_or_else(|| AppError::CapabilityNotFound {
                capability_id: capability_id.to_string(),
                environment: environment.to_owned(),
            })
    }

    pub fn import_github_session(
        &self,
        project_home: PathBuf,
    ) -> Result<GitHubSessionView, AppError> {
        let token = import_gh_token()?;
        self.import_github_token(project_home, token)
    }

    pub fn import_github_token(
        &self,
        project_home: PathBuf,
        token: GitHubToken,
    ) -> Result<GitHubSessionView, AppError> {
        let client = self.github.as_ref().ok_or(AppError::GitHubNotConfigured)?;
        let connection = client.authenticated_user(&token)?;
        let mut project = OpenProject::open(project_home)?;
        let metadata = self.persist_credential(
            &mut project,
            format!("GitHub · {}", connection.login),
            GITHUB_PROVIDER.to_owned(),
            CredentialKind::OAuthToken,
            CredentialEnvironment::Shared,
            token.expose_secret(),
        )?;
        Ok(GitHubSessionView {
            credential_id: metadata.id,
            credential_ref: metadata.secret_ref,
            login: connection.login,
            name: connection.name,
            avatar_url: connection.avatar_url,
            scopes: connection.scopes,
        })
    }

    pub fn start_github_device_flow(&self, client_id: &str) -> Result<DeviceFlowView, AppError> {
        let client = self.github.as_ref().ok_or(AppError::GitHubNotConfigured)?;
        let flow = client.start_device_flow(client_id, &["repo", "read:org"])?;
        let session_id = Uuid::new_v4().to_string();
        let mut flows = self
            .pending_device_flows
            .lock()
            .map_err(|_| AppError::Integration("device flow state was poisoned".to_owned()))?;
        flows.insert(
            session_id.clone(),
            PendingDeviceFlow {
                client_id: client_id.trim().to_owned(),
                device_code: flow.device_code().to_owned(),
            },
        );
        Ok(DeviceFlowView {
            session_id,
            user_code: flow.user_code,
            verification_uri: flow.verification_uri,
            expires_in: flow.expires_in,
            interval: flow.interval,
        })
    }

    pub fn poll_github_device_flow(
        &self,
        project_home: PathBuf,
        session_id: &str,
    ) -> Result<GitHubDevicePollView, AppError> {
        let client = self.github.as_ref().ok_or(AppError::GitHubNotConfigured)?;
        let pending = self
            .pending_device_flows
            .lock()
            .map_err(|_| AppError::Integration("device flow state was poisoned".to_owned()))?
            .get(session_id)
            .cloned()
            .ok_or_else(|| AppError::DeviceFlowNotFound(session_id.to_owned()))?;
        let poll = client.poll_device_flow(&pending.client_id, &pending.device_code)?;
        match poll {
            DeviceFlowPoll::Pending { interval } => Ok(GitHubDevicePollView::Pending {
                session_id: session_id.to_owned(),
                interval,
            }),
            DeviceFlowPoll::SlowDown { interval } => Ok(GitHubDevicePollView::SlowDown {
                session_id: session_id.to_owned(),
                interval,
            }),
            DeviceFlowPoll::AccessDenied => {
                self.remove_device_flow(session_id)?;
                Ok(GitHubDevicePollView::AccessDenied)
            }
            DeviceFlowPoll::Expired => {
                self.remove_device_flow(session_id)?;
                Ok(GitHubDevicePollView::Expired)
            }
            DeviceFlowPoll::Authorized { token, .. } => {
                self.remove_device_flow(session_id)?;
                let session = self.import_github_token(project_home, token)?;
                Ok(GitHubDevicePollView::Authorized { session })
            }
        }
    }

    pub fn create_github_private_repository(
        &self,
        project_home: PathBuf,
        input: CreatePrivateRepositoryInput,
    ) -> Result<GitHubRepository, AppError> {
        let mut project = OpenProject::open(project_home)?;
        let repository = project
            .database
            .repository_by_key(&project.project.id, &input.repository_key)?
            .ok_or_else(|| AppError::RepositoryNotFound(input.repository_key.clone()))?;
        let credential = project
            .database
            .credential(&project.project.id, &input.credential_id)?
            .ok_or(AppError::CredentialNotFound(input.credential_id))?;
        let token = GitHubToken::new(self.secrets.get(&credential.secret_ref)?)?;
        let client = self.github.as_ref().ok_or(AppError::GitHubNotConfigured)?;
        let created = client.create_private_repository(
            &token,
            &CreatePrivateRepositoryRequest {
                name: input.name,
                description: input.description,
            },
        )?;
        self.git
            .set_remote(&repository.local_path, "origin", &created.clone_url)?;
        let mut updated = repository;
        updated.remote_url = created.clone_url.clone();
        updated.updated_at = Utc::now();
        project.database.save_repository(&updated)?;
        Ok(created)
    }

    fn validate_git_repository(
        &self,
        repository: &Repository,
    ) -> Result<GitRepositoryStatus, AppError> {
        let status = self.git.inspect(&repository.local_path)?;
        self.git
            .validate_remote(&repository.local_path, "origin", &repository.remote_url)?;
        Ok(status)
    }

    #[allow(clippy::too_many_arguments)]
    fn persist_credential(
        &self,
        project: &mut OpenProject,
        name: String,
        provider: String,
        kind: CredentialKind,
        environment: CredentialEnvironment,
        secret: &str,
    ) -> Result<CredentialMetadata, AppError> {
        let mut metadata = CredentialMetadata::new(
            project.project.id,
            name,
            provider,
            kind,
            environment,
            "coldstart:credential:pending".to_owned(),
        )?;
        metadata.secret_ref = format!("coldstart:credential:{}", metadata.id);
        metadata.validate()?;
        self.secrets.set(&metadata.secret_ref, secret)?;
        if let Err(error) = project.database.save_credential(&metadata) {
            let _ = self.secrets.delete(&metadata.secret_ref);
            return Err(error.into());
        }
        Ok(metadata)
    }

    fn remove_device_flow(&self, session_id: &str) -> Result<(), AppError> {
        self.pending_device_flows
            .lock()
            .map_err(|_| AppError::Integration("device flow state was poisoned".to_owned()))?
            .remove(session_id);
        Ok(())
    }
}

impl Default for IntegrationService {
    fn default() -> Self {
        Self::new()
    }
}

fn build_integration_workspace(project: &OpenProject) -> Result<IntegrationWorkspace, AppError> {
    let repositories = project.database.list_repositories(&project.project.id)?;
    let verification_commands = repositories
        .iter()
        .map(|repository| {
            Ok(RepositoryVerificationView {
                repository_key: repository.key.clone(),
                commands: project
                    .database
                    .list_repository_verification_commands(&project.project.id, &repository.key)?,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    let credentials = project.database.list_credentials(&project.project.id)?;
    let capabilities = load_capability_views(project)?;
    let git_syncs = project.database.list_git_syncs(&project.project.id)?;
    let plan = build_plan_workspace(project)?;
    let release = ReleaseCenterView {
        prerequisites: plan
            .work_items
            .iter()
            .filter(|view| view.task.task_key.starts_with("release-prereq:"))
            .cloned()
            .collect(),
        milestones: plan
            .work_items
            .iter()
            .filter(|view| view.task.task_key.starts_with("release:"))
            .cloned()
            .collect(),
        approval_gates: plan
            .work_items
            .iter()
            .filter(|view| view.task.kind == TaskKind::ApprovalGate)
            .cloned()
            .collect(),
        human_tasks: plan
            .work_items
            .iter()
            .filter(|view| {
                view.task.kind == TaskKind::HumanExternal
                    && !view.task.task_key.starts_with("release-prereq:")
            })
            .cloned()
            .collect(),
    };
    Ok(IntegrationWorkspace {
        project_id: project.project.id,
        repositories,
        verification_commands,
        credentials,
        capabilities,
        git_syncs,
        release,
    })
}

fn verification_storage_error(error: StorageError) -> AppError {
    match error {
        StorageError::VerificationCommandInvalid(message) => {
            AppError::VerificationCommandInvalid(message)
        }
        StorageError::VerificationNotConfigured(work_item_id) => {
            AppError::VerificationNotConfigured(work_item_id)
        }
        StorageError::VerificationInProgress(work_item_id) => {
            AppError::VerificationInProgress(work_item_id)
        }
        other => other.into(),
    }
}

fn sync_capabilities_from_blueprint(project: &mut OpenProject) -> Result<(), AppError> {
    let Some(blueprint) = project.database.latest_blueprint(
        &project.project.id,
        coldstart_core::BlueprintStatus::Published,
    )?
    else {
        return Ok(());
    };
    let existing = project
        .database
        .list_capability_states(&project.project.id)?;
    let existing = existing
        .into_iter()
        .map(|state| {
            (
                (state.capability_id.to_string(), state.environment.clone()),
                state,
            )
        })
        .collect::<BTreeMap<_, _>>();
    for capability in blueprint.content.capabilities {
        for environment in CAPABILITY_ENVIRONMENTS {
            let key = (capability.id.to_string(), environment.to_owned());
            if let Some(state) = existing.get(&key) {
                if state.name != capability.name || state.provider != capability.provider {
                    let mut updated = state.clone();
                    updated.name = capability.name.clone();
                    updated.provider = capability.provider.clone();
                    updated.updated_at = Utc::now();
                    project.database.save_capability_state(&updated)?;
                }
                continue;
            }
            let state = CapabilityState::new(
                coldstart_core::CapabilityDescriptor {
                    project_id: project.project.id,
                    capability_id: capability.id.clone(),
                    name: capability.name.clone(),
                    provider: capability.provider.clone(),
                    environment: environment.to_owned(),
                },
                CapabilityStatus::Missing,
                None,
                None,
            )?;
            project.database.save_capability_state(&state)?;
        }
    }
    Ok(())
}

fn load_capability_views(project: &OpenProject) -> Result<Vec<CapabilityStatusView>, AppError> {
    let credentials = project.database.list_credentials(&project.project.id)?;
    let credential_refs = credentials
        .into_iter()
        .map(|credential| (credential.id, credential.secret_ref))
        .collect::<BTreeMap<_, _>>();
    Ok(project
        .database
        .list_capability_states(&project.project.id)?
        .into_iter()
        .map(|state| CapabilityStatusView {
            project_id: state.project_id,
            capability_id: state.capability_id,
            name: state.name,
            provider: state.provider,
            environment: state.environment,
            status: state.status,
            credential_id: state.credential_id,
            credential_ref: state
                .credential_id
                .and_then(|id| credential_refs.get(&id).cloned()),
            note: state.note,
            updated_at: state.updated_at,
        })
        .collect())
}

pub(crate) fn git_auth_for_repository(
    project: &OpenProject,
    repository: &Repository,
    secrets: &dyn SecretStore,
) -> Result<coldstart_adapters::GitAuth, AppError> {
    match repository.auth_mode {
        RepositoryAuthMode::None => Ok(coldstart_adapters::GitAuth::None),
        RepositoryAuthMode::SshAgent => Ok(coldstart_adapters::GitAuth::SshAgent),
        RepositoryAuthMode::Credential => {
            let credential_id = repository
                .credential_id
                .ok_or(AppError::RepositoryCredentialMissing(repository.id))?;
            let credential = project
                .database
                .credential(&project.project.id, &credential_id)?
                .ok_or(AppError::CredentialNotFound(credential_id))?;
            let secret = secrets.get(&credential.secret_ref)?;
            Ok(coldstart_adapters::GitAuth::github(GitHubToken::new(
                secret,
            )?))
        }
    }
}

pub(crate) fn load_capabilities_for_context(
    project: &OpenProject,
) -> Result<Vec<CapabilityStatusView>, AppError> {
    load_capability_views(project)
}
