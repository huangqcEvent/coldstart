/**
 * 仓库、凭据、能力与能力 API 白名单 API。
 *
 * Secret 只提交给 Rust 端写入系统凭据库，读取结果只包含引用和状态，不回显明文。
 */
import { invoke } from '@tauri-apps/api/core';

import type {
  CapabilityApiEndpointInput,
  CapabilityStatus,
  CreatePrivateRepositoryInput,
  GitHubDeviceFlow,
  GitHubDevicePoll,
  GitHubRepository,
  GitHubSession,
  IntegrationWorkspace,
  RepositoryDraft,
  RepositoryVerificationCommand,
  RepositoryValidation,
  SaveCredentialInput,
} from './types';

export async function readIntegrationWorkspace(path: string) {
  return invoke<IntegrationWorkspace>('read_integration_workspace', { path });
}

export async function validateRepository(path: string, draft: RepositoryDraft) {
  return invoke<RepositoryValidation>('validate_repository', { path, draft });
}

export async function saveRepository(path: string, draft: RepositoryDraft) {
  return invoke<IntegrationWorkspace>('save_repository', { path, draft });
}

export async function readRepositoryVerificationCommands(
  path: string,
  repositoryKey: string,
) {
  return invoke<RepositoryVerificationCommand[]>(
    'read_repository_verification_commands',
    { path, repositoryKey },
  );
}

export async function saveRepositoryVerificationCommands(
  path: string,
  repositoryKey: string,
  commands: RepositoryVerificationCommand[],
) {
  return invoke<RepositoryVerificationCommand[]>(
    'save_repository_verification_commands',
    { path, repositoryKey, commands },
  );
}

export async function saveCredential(path: string, input: SaveCredentialInput) {
  return invoke<IntegrationWorkspace>('save_credential', { path, input });
}

export async function deleteCredential(path: string, credentialId: string) {
  return invoke<IntegrationWorkspace>('delete_credential', {
    path,
    credentialId,
  });
}

export async function updateCapability(
  path: string,
  capabilityId: string,
  environment: string,
  status: CapabilityStatus,
  credentialId: string | null,
  note: string | null,
) {
  return invoke<IntegrationWorkspace>('update_capability', {
    path,
    capabilityId,
    environment,
    status,
    credentialId,
    note,
  });
}

export async function saveCapabilityApiEndpoint(
  path: string,
  endpointId: string | null,
  input: CapabilityApiEndpointInput,
) {
  return invoke<IntegrationWorkspace>('save_capability_api_endpoint', {
    path,
    endpointId,
    input,
  });
}

export async function deleteCapabilityApiEndpoint(
  path: string,
  endpointId: string,
) {
  return invoke<IntegrationWorkspace>('delete_capability_api_endpoint', {
    path,
    endpointId,
  });
}

export async function importGitHubSession(path: string) {
  return invoke<GitHubSession>('import_github_session', { path });
}

export async function startGitHubDeviceFlow(clientId: string) {
  return invoke<GitHubDeviceFlow>('start_github_device_flow', {
    clientId,
  });
}

export async function pollGitHubDeviceFlow(path: string, sessionId: string) {
  return invoke<GitHubDevicePoll>('poll_github_device_flow', {
    path,
    sessionId,
  });
}

export async function createGitHubPrivateRepository(
  path: string,
  input: CreatePrivateRepositoryInput,
) {
  return invoke<GitHubRepository>('create_github_private_repository', {
    path,
    input,
  });
}
