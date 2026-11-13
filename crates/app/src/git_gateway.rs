//! 计划执行所需的 Git 网关抽象。
//!
//! 应用层通过该接口完成分支创建、提交校验、差异读取和推送，测试可替换为内存实现；
//! 真实实现仍由适配器落实认证与命令安全约束。

use std::path::Path;

use coldstart_adapters::{GitAdapter, GitAuth, GitCommitVerification, GitError, ReviewDiff};

pub trait PlanGitGateway: Send + Sync {
    fn fetch(&self, repository: &Path, auth: &GitAuth) -> Result<(), GitError>;

    fn resolve_commit(&self, repository: &Path, reference: &str) -> Result<String, GitError>;

    fn validate_remote(
        &self,
        repository: &Path,
        remote_name: &str,
        expected_url: &str,
    ) -> Result<(), GitError>;

    fn create_work_branch(
        &self,
        repository: &Path,
        base_ref: &str,
        branch: &str,
        protected_branches: &[String],
        auth: &GitAuth,
    ) -> Result<(), GitError>;

    #[allow(clippy::too_many_arguments)]
    fn verify_report_commit(
        &self,
        repository: &Path,
        branch: &str,
        base_ref: &str,
        commit_sha: &str,
        allowed_paths: &[String],
        protected_branches: &[String],
    ) -> Result<GitCommitVerification, GitError>;

    fn ensure_verification_checkout(
        &self,
        repository: &Path,
        branch: &str,
        commit_sha: &str,
    ) -> Result<(), GitError>;

    fn extract_review_diff(
        &self,
        repository: &Path,
        base_commit: &str,
        head_commit: &str,
    ) -> Result<ReviewDiff, GitError>;

    fn push_branch(
        &self,
        repository: &Path,
        branch: &str,
        protected_branches: &[String],
        auth: &GitAuth,
    ) -> Result<(), GitError>;
}

impl PlanGitGateway for GitAdapter {
    fn fetch(&self, repository: &Path, auth: &GitAuth) -> Result<(), GitError> {
        GitAdapter::fetch(self, repository, auth)
    }

    fn resolve_commit(&self, repository: &Path, reference: &str) -> Result<String, GitError> {
        GitAdapter::resolve_commit(self, repository, reference)
    }

    fn validate_remote(
        &self,
        repository: &Path,
        remote_name: &str,
        expected_url: &str,
    ) -> Result<(), GitError> {
        GitAdapter::validate_remote(self, repository, remote_name, expected_url)
    }

    fn create_work_branch(
        &self,
        repository: &Path,
        base_ref: &str,
        branch: &str,
        protected_branches: &[String],
        auth: &GitAuth,
    ) -> Result<(), GitError> {
        GitAdapter::create_work_branch(self, repository, base_ref, branch, protected_branches, auth)
    }

    fn verify_report_commit(
        &self,
        repository: &Path,
        branch: &str,
        base_ref: &str,
        commit_sha: &str,
        allowed_paths: &[String],
        protected_branches: &[String],
    ) -> Result<GitCommitVerification, GitError> {
        GitAdapter::verify_report_commit(
            self,
            repository,
            branch,
            base_ref,
            commit_sha,
            allowed_paths,
            protected_branches,
        )
    }

    fn ensure_verification_checkout(
        &self,
        repository: &Path,
        branch: &str,
        commit_sha: &str,
    ) -> Result<(), GitError> {
        GitAdapter::ensure_verification_checkout(self, repository, branch, commit_sha)
    }

    fn extract_review_diff(
        &self,
        repository: &Path,
        base_commit: &str,
        head_commit: &str,
    ) -> Result<ReviewDiff, GitError> {
        GitAdapter::extract_review_diff(self, repository, base_commit, head_commit)
    }

    fn push_branch(
        &self,
        repository: &Path,
        branch: &str,
        protected_branches: &[String],
        auth: &GitAuth,
    ) -> Result<(), GitError> {
        GitAdapter::push_branch(self, repository, branch, protected_branches, auth)
    }
}
