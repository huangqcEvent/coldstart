//! 项目事件的只读投影与分页读取。
//!
//! 事件本身只追加、不修改；这里负责把事件载荷转换为项目记忆所需的安全视图，
//! 并对分页、筛选和展示字段做边界控制。

use chrono::{DateTime, Utc};
use coldstart_core::{DomainEvent, DomainEventKind, ProjectId};
use rusqlite::params;
use uuid::Uuid;

use crate::{Database, StorageError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredEvent {
    pub seq: i64,
    pub event: DomainEvent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventPage {
    pub events: Vec<StoredEvent>,
    pub next_cursor: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventTypeCount {
    pub event_type: String,
    pub count: u64,
}

impl Database {
    /// 按项目、阶段和时间分页读取事件，并投影为安全时间线项。
    pub fn list_events(
        &self,
        project_id: &ProjectId,
        before_seq: Option<i64>,
        limit: u32,
        include_heartbeats: bool,
    ) -> Result<EventPage, StorageError> {
        let fetch_limit = i64::from(limit).saturating_add(1);
        let mut statement = self.connection().prepare(
            "SELECT seq, event_id, project_id, occurred_at, event_type, payload_json
             FROM events
             WHERE project_id = ?1
               AND (?2 IS NULL OR seq < ?2)
               AND (?3 = 1 OR event_type <> 'work_heartbeat')
             ORDER BY seq DESC
             LIMIT ?4",
        )?;
        let rows = statement.query_map(
            params![
                project_id.to_string(),
                before_seq,
                include_heartbeats,
                fetch_limit
            ],
            |row| {
                Ok(StoredEventRow {
                    seq: row.get(0)?,
                    event_id: row.get(1)?,
                    project_id: row.get(2)?,
                    occurred_at: row.get(3)?,
                    event_type: row.get(4)?,
                    payload_json: row.get(5)?,
                })
            },
        )?;
        let mut rows = rows.collect::<Result<Vec<_>, _>>()?;
        let has_more = rows.len() > limit as usize;
        if has_more {
            rows.truncate(limit as usize);
        }
        let next_cursor = has_more.then(|| rows.last().map(|row| row.seq)).flatten();
        let events = rows
            .into_iter()
            .map(StoredEventRow::into_event)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(EventPage {
            events,
            next_cursor,
        })
    }

    pub fn event_type_counts(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<EventTypeCount>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT event_type, COUNT(*)
             FROM events
             WHERE project_id = ?1
             GROUP BY event_type
             ORDER BY event_type",
        )?;
        let rows = statement.query_map([project_id.to_string()], |row| {
            Ok(EventTypeCount {
                event_type: row.get(0)?,
                count: row.get::<_, i64>(1)? as u64,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }
}

struct StoredEventRow {
    seq: i64,
    event_id: String,
    project_id: String,
    occurred_at: String,
    event_type: String,
    payload_json: String,
}

impl StoredEventRow {
    fn into_event(self) -> Result<StoredEvent, StorageError> {
        let event_id = Uuid::parse_str(&self.event_id)?;
        let kind: DomainEventKind = serde_json::from_str(&self.payload_json)?;
        if kind.type_name() != self.event_type {
            return Err(StorageError::EventTypeMismatch {
                event_id,
                stored_type: self.event_type,
                payload_type: kind.type_name().to_owned(),
            });
        }

        Ok(StoredEvent {
            seq: self.seq,
            event: DomainEvent {
                event_id,
                project_id: self.project_id.parse::<ProjectId>()?,
                occurred_at: DateTime::parse_from_rfc3339(&self.occurred_at)?.with_timezone(&Utc),
                kind,
            },
        })
    }
}
