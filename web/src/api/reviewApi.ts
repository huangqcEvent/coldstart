import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';

import type {
  ReviewConfiguration,
  ReviewConfigurationInput,
  ReviewStandardContent,
  ReviewStandardImport,
  ReviewStandardManualInput,
  ReviewStandardRevisionInput,
  ReviewStandardView,
  ReviewWorkspace,
  PullRequestRecord,
  WorkReportRecord,
} from './types';

export async function readReviewWorkspace(path: string) {
  return invoke<ReviewWorkspace>('load_review_workspace', { path });
}

export async function saveReviewConfiguration(
  path: string,
  input: ReviewConfigurationInput,
) {
  return invoke<ReviewConfiguration>('save_review_configuration', {
    path,
    input,
  });
}

export async function importReviewStandard(
  path: string,
  input: ReviewStandardImport,
) {
  return invoke<ReviewStandardView>('import_review_standard', { path, input });
}

export async function createReviewStandard(
  path: string,
  input: ReviewStandardManualInput,
) {
  return invoke<ReviewStandardView>('create_review_standard', { path, input });
}

export async function reviseReviewStandard(
  path: string,
  standardId: string,
  input: ReviewStandardRevisionInput,
) {
  return invoke<ReviewStandardView>('revise_review_standard', {
    path,
    standardId,
    input,
  });
}

export async function setReviewStandardEnabled(
  path: string,
  standardId: string,
  enabled: boolean,
) {
  return invoke<ReviewWorkspace>('set_review_standard_enabled', {
    path,
    standardId,
    enabled,
  });
}

export async function reorderReviewStandards(
  path: string,
  orderedIds: string[],
) {
  return invoke<ReviewWorkspace>('reorder_review_standards', {
    path,
    orderedIds,
  });
}

export async function deleteReviewStandard(path: string, standardId: string) {
  return invoke<ReviewWorkspace>('delete_review_standard', {
    path,
    standardId,
  });
}

export async function readReviewStandard(path: string, standardId: string) {
  return invoke<ReviewStandardContent>('read_review_standard', {
    path,
    standardId,
  });
}

export async function waiveWorkReview(
  path: string,
  workItemId: string,
  reportId: string,
  reason: string,
) {
  return invoke<WorkReportRecord>('waive_work_review', {
    path,
    workItemId,
    reportId,
    reason,
  });
}

export async function createDraftPullRequest(path: string, reportId: string) {
  return invoke<PullRequestRecord>('create_draft_pull_request', {
    path,
    reportId,
  });
}

export async function refreshPullRequest(path: string, pullRequestId: string) {
  return invoke<PullRequestRecord>('refresh_pull_request', {
    path,
    pullRequestId,
  });
}

export async function pickReviewStandardFile(): Promise<string | null> {
  const selected = await open({
    multiple: false,
    directory: false,
    title: '选择代码规范文件',
    filters: [
      {
        name: '代码规范',
        extensions: ['md', 'markdown', 'txt', 'docx', 'pdf'],
      },
    ],
  });
  return typeof selected === 'string' ? selected : null;
}
