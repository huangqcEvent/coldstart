use std::str::FromStr;

use chrono::{DateTime, Utc};
use coldstart_core::{
    CapabilityApiEndpoint, CapabilityApiEndpointId, CapabilityApiMethod, ProjectId,
};
use rusqlite::{OptionalExtension, Row, TransactionBehavior, params};

use crate::{Database, StorageError, database::project_exists};

impl Database {
    pub fn save_capability_api_endpoint(
        &mut self,
        endpoint: &CapabilityApiEndpoint,
    ) -> Result<(), StorageError> {
        endpoint.validate()?;
        let allowed_methods_json = serde_json::to_string(&endpoint.allowed_methods)?;
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !project_exists(&transaction, &endpoint.project_id)? {
            return Err(StorageError::ProjectNotFound(endpoint.project_id));
        }
        if let Some(credential_id) = endpoint.credential_id {
            let credential_in_project: bool = transaction.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM credentials WHERE id = ?1 AND project_id = ?2
                 )",
                params![credential_id.to_string(), endpoint.project_id.to_string()],
                |row| row.get(0),
            )?;
            if !credential_in_project {
                return Err(StorageError::CredentialProjectMismatch(credential_id));
            }
        }
        let capability_exists: bool = transaction.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM capability_states
                WHERE project_id = ?1 AND capability_id = ?2 AND environment = ?3
             )",
            params![
                endpoint.project_id.to_string(),
                endpoint.capability_id.as_str(),
                endpoint.environment,
            ],
            |row| row.get(0),
        )?;
        if !capability_exists {
            return Err(StorageError::CapabilityApiCapabilityNotFound {
                capability_id: endpoint.capability_id.to_string(),
                environment: endpoint.environment.clone(),
            });
        }
        if let Some(existing_project) = transaction
            .query_row(
                "SELECT project_id FROM capability_api_endpoints WHERE id = ?1",
                [endpoint.id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            && existing_project != endpoint.project_id.to_string()
        {
            return Err(StorageError::CapabilityApiEndpointProjectMismatch(
                endpoint.id,
            ));
        }
        if let Some(existing_id) = transaction
            .query_row(
                "SELECT id FROM capability_api_endpoints
                 WHERE project_id = ?1 AND capability_id = ?2 AND environment = ?3 AND name = ?4",
                params![
                    endpoint.project_id.to_string(),
                    endpoint.capability_id.as_str(),
                    endpoint.environment,
                    endpoint.name,
                ],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            && existing_id != endpoint.id.to_string()
        {
            return Err(StorageError::CapabilityApiEndpointNameAlreadyExists {
                capability_id: endpoint.capability_id.to_string(),
                environment: endpoint.environment.clone(),
                name: endpoint.name.clone(),
            });
        }
        let created_at = transaction
            .query_row(
                "SELECT created_at FROM capability_api_endpoints WHERE id = ?1",
                [endpoint.id.to_string()],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .unwrap_or_else(|| endpoint.created_at.to_rfc3339());
        transaction.execute(
            "INSERT INTO capability_api_endpoints (
                id, project_id, capability_id, environment, name, base_url,
                path_prefix, allowed_methods_json, credential_id, auth_header,
                auth_scheme, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(id) DO UPDATE SET
                capability_id = excluded.capability_id,
                environment = excluded.environment,
                name = excluded.name,
                base_url = excluded.base_url,
                path_prefix = excluded.path_prefix,
                allowed_methods_json = excluded.allowed_methods_json,
                credential_id = excluded.credential_id,
                auth_header = excluded.auth_header,
                auth_scheme = excluded.auth_scheme,
                updated_at = excluded.updated_at",
            params![
                endpoint.id.to_string(),
                endpoint.project_id.to_string(),
                endpoint.capability_id.as_str(),
                endpoint.environment,
                endpoint.name,
                endpoint.base_url,
                endpoint.path_prefix,
                allowed_methods_json,
                endpoint.credential_id.map(|id| id.to_string()),
                endpoint.auth_header,
                endpoint.auth_scheme,
                created_at,
                endpoint.updated_at.to_rfc3339(),
            ],
        )?;
        transaction.commit()?;
        Ok(())
    }

    pub fn list_capability_api_endpoints(
        &self,
        project_id: &ProjectId,
    ) -> Result<Vec<CapabilityApiEndpoint>, StorageError> {
        let mut statement = self.connection().prepare(
            "SELECT id, project_id, capability_id, environment, name, base_url,
                    path_prefix, allowed_methods_json, credential_id, auth_header,
                    auth_scheme, created_at, updated_at
             FROM capability_api_endpoints WHERE project_id = ?1
             ORDER BY environment ASC, capability_id ASC, name ASC",
        )?;
        let rows = statement.query_map([project_id.to_string()], read_endpoint_row)?;
        let mut endpoints = Vec::new();
        for row in rows {
            endpoints.push(row?.into_endpoint()?);
        }
        Ok(endpoints)
    }

    pub fn capability_api_endpoint(
        &self,
        project_id: &ProjectId,
        endpoint_id: &CapabilityApiEndpointId,
    ) -> Result<Option<CapabilityApiEndpoint>, StorageError> {
        let row = self
            .connection()
            .query_row(
                "SELECT id, project_id, capability_id, environment, name, base_url,
                        path_prefix, allowed_methods_json, credential_id, auth_header,
                        auth_scheme, created_at, updated_at
                 FROM capability_api_endpoints WHERE project_id = ?1 AND id = ?2",
                params![project_id.to_string(), endpoint_id.to_string()],
                read_endpoint_row,
            )
            .optional()?;
        row.map(StoredEndpointRow::into_endpoint).transpose()
    }

    pub fn delete_capability_api_endpoint(
        &mut self,
        project_id: &ProjectId,
        endpoint_id: &CapabilityApiEndpointId,
    ) -> Result<bool, StorageError> {
        let transaction = self
            .connection_mut()
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let deleted = transaction.execute(
            "DELETE FROM capability_api_endpoints WHERE project_id = ?1 AND id = ?2",
            params![project_id.to_string(), endpoint_id.to_string()],
        )?;
        transaction.commit()?;
        Ok(deleted > 0)
    }
}

#[derive(Debug)]
struct StoredEndpointRow {
    id: String,
    project_id: String,
    capability_id: String,
    environment: String,
    name: String,
    base_url: String,
    path_prefix: String,
    allowed_methods_json: String,
    credential_id: Option<String>,
    auth_header: Option<String>,
    auth_scheme: Option<String>,
    created_at: String,
    updated_at: String,
}

impl StoredEndpointRow {
    fn into_endpoint(self) -> Result<CapabilityApiEndpoint, StorageError> {
        Ok(CapabilityApiEndpoint {
            id: parse_id(&self.id, "capability API endpoint")?,
            project_id: parse_id(&self.project_id, "project")?,
            capability_id: parse_id(&self.capability_id, "capability")?,
            environment: self.environment,
            name: self.name,
            base_url: self.base_url,
            path_prefix: self.path_prefix,
            allowed_methods: serde_json::from_str::<Vec<CapabilityApiMethod>>(
                &self.allowed_methods_json,
            )?,
            credential_id: self
                .credential_id
                .map(|id| parse_id(&id, "credential"))
                .transpose()?,
            auth_header: self.auth_header,
            auth_scheme: self.auth_scheme,
            created_at: parse_date(&self.created_at)?,
            updated_at: parse_date(&self.updated_at)?,
        })
    }
}

fn read_endpoint_row(row: &Row<'_>) -> rusqlite::Result<StoredEndpointRow> {
    Ok(StoredEndpointRow {
        id: row.get(0)?,
        project_id: row.get(1)?,
        capability_id: row.get(2)?,
        environment: row.get(3)?,
        name: row.get(4)?,
        base_url: row.get(5)?,
        path_prefix: row.get(6)?,
        allowed_methods_json: row.get(7)?,
        credential_id: row.get(8)?,
        auth_header: row.get(9)?,
        auth_scheme: row.get(10)?,
        created_at: row.get(11)?,
        updated_at: row.get(12)?,
    })
}

fn parse_id<T>(value: &str, kind: &'static str) -> Result<T, StorageError>
where
    T: FromStr,
    T::Err: std::fmt::Display,
{
    value.parse().map_err(|_| StorageError::InvalidId {
        kind,
        value: value.to_owned(),
    })
}

fn parse_date(value: &str) -> Result<DateTime<Utc>, StorageError> {
    Ok(DateTime::parse_from_rfc3339(value)?.with_timezone(&Utc))
}
