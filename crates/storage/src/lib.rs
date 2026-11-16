//! ColdStart SQLite 存储层。
//!
//! 对外暴露数据库打开、领域存储和迁移入口；SQL、事务和行映射封装在本 crate
//! 内部，应用层只通过稳定方法操作状态。

mod capability_api_store;
mod database;
mod event_store;
mod integration_store;
mod investigation_store;
mod migrations;
mod plan_store;
mod pull_request_store;
mod review_store;
mod verification_store;

pub use database::{Database, StorageError};
pub use event_store::{EventPage, EventTypeCount, StoredEvent};
pub use plan_store::NewWorkBlocker;
pub use review_store::{BeginReview, NewReviewStandardRevision, PendingReview};
pub use verification_store::{BeginVerification, PendingVerification};
