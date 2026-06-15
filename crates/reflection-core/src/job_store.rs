use std::{collections::HashMap, path::Path};

use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{
    models::{
        ApiKeyRecord, ApiKeyRole, ApiKeyView, ArtifactView, AuthMode, CandidateKind,
        CandidateProtection, CandidateValidationState, ClearJobsResponse, CreateUserKeyRequest,
        CreatedUserKeyResponse, DiscoveryMode, HiddenJobBatchView, JobRecord, JobStatus,
        MediaCandidate, OutputKind, PlatformHint, RestoreJobsResponse, RotatedAdminKeyResponse,
        UpdateRuntimeSettingsRequest,
    },
    observability::{
        BrowserSession, DomainPolicy, ErrorClass, JobTrace, MediaProbe, PipelineEvent,
        PipelineEventType, RequestLog, RequestPhase, TranscodeRun,
    },
    Result, RkError,
};

#[derive(Debug, Clone)]
pub struct JobStore {
    pool: SqlitePool,
}

impl JobStore {
    pub async fn connect(database_path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    pub async fn insert(&self, record: &JobRecord) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO jobs (
                id,
                status,
                source_url,
                bitrate,
                created_at,
                updated_at,
                status_url,
                media_url,
                error,
                discovery,
                platform_hint,
                outputs_json,
                profile_id,
                auth_mode,
                selected_candidate_ids_json,
                requester_ip,
                requester_user_agent,
                requester_label,
                requester_key_id,
                resolved_extractor,
                error_class,
                attempt_count,
                started_at,
                completed_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(record.id.to_string())
        .bind(record.status.as_str())
        .bind(&record.source_url)
        .bind(&record.bitrate)
        .bind(format_time(record.created_at)?)
        .bind(format_time(record.updated_at)?)
        .bind(&record.status_url)
        .bind(&record.media_url)
        .bind(&record.error)
        .bind(record.discovery.as_str())
        .bind(record.platform_hint.as_str())
        .bind(serde_json::to_string(&record.outputs)?)
        .bind(&record.profile_id)
        .bind(record.auth_mode.as_str())
        .bind(serde_json::to_string(&record.selected_candidate_ids)?)
        .bind(&record.requester_ip)
        .bind(&record.requester_user_agent)
        .bind(&record.requester_label)
        .bind(record.requester_key_id.map(|id| id.to_string()))
        .bind(&record.resolved_extractor)
        .bind(record.error_class.as_str())
        .bind(record.attempt_count)
        .bind(record.started_at.map(format_time).transpose()?)
        .bind(record.completed_at.map(format_time).transpose()?)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn get(&self, id: Uuid) -> Result<Option<JobRecord>> {
        let Some(row) = sqlx::query(
            r#"
            SELECT id,
                   status,
                   source_url,
                   bitrate,
                   created_at,
                   updated_at,
                   status_url,
                   media_url,
                   error,
                   discovery,
                   platform_hint,
                   outputs_json,
                   profile_id,
                   auth_mode,
                   selected_candidate_ids_json,
                   requester_ip,
                   requester_user_agent,
                   requester_label,
                   requester_key_id,
                   resolved_extractor,
                   error_class,
                   attempt_count,
                   started_at,
                   completed_at
            FROM jobs
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(row_to_job(row)?))
    }

    pub async fn list_recent(&self, limit: usize) -> Result<Vec<JobRecord>> {
        let limit = limit.clamp(1, 200) as i64;
        let rows = sqlx::query(
            r#"
            SELECT id,
                   status,
                   source_url,
                   bitrate,
                   created_at,
                   updated_at,
                   status_url,
                   media_url,
                   error,
                   discovery,
                   platform_hint,
                   outputs_json,
                   profile_id,
                   auth_mode,
                   selected_candidate_ids_json,
                   requester_ip,
                   requester_user_agent,
                   requester_label,
                   requester_key_id,
                   resolved_extractor,
                   error_class,
                   attempt_count,
                   started_at,
                   completed_at
            FROM jobs
            WHERE hidden_at IS NULL
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_job).collect()
    }

    pub async fn list_recent_for_key(
        &self,
        requester_key_id: Uuid,
        limit: usize,
    ) -> Result<Vec<JobRecord>> {
        let limit = limit.clamp(1, 200) as i64;
        let rows = sqlx::query(
            r#"
            SELECT id,
                   status,
                   source_url,
                   bitrate,
                   created_at,
                   updated_at,
                   status_url,
                   media_url,
                   error,
                   discovery,
                   platform_hint,
                   outputs_json,
                   profile_id,
                   auth_mode,
                   selected_candidate_ids_json,
                   requester_ip,
                   requester_user_agent,
                   requester_label,
                   requester_key_id,
                   resolved_extractor,
                   error_class,
                   attempt_count,
                   started_at,
                   completed_at
            FROM jobs
            WHERE requester_key_id = ? AND hidden_at IS NULL
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(requester_key_id.to_string())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_job).collect()
    }

    pub async fn hide_visible_jobs(
        &self,
        actor_key_id: Option<Uuid>,
        actor_label: Option<&str>,
    ) -> Result<ClearJobsResponse> {
        self.hide_visible_jobs_inner(actor_key_id, actor_label, None)
            .await
    }

    pub async fn hide_visible_jobs_for_key(
        &self,
        requester_key_id: Uuid,
        actor_label: Option<&str>,
    ) -> Result<ClearJobsResponse> {
        self.hide_visible_jobs_inner(Some(requester_key_id), actor_label, Some(requester_key_id))
            .await
    }

    async fn hide_visible_jobs_inner(
        &self,
        actor_key_id: Option<Uuid>,
        actor_label: Option<&str>,
        requester_key_id: Option<Uuid>,
    ) -> Result<ClearJobsResponse> {
        let now = OffsetDateTime::now_utc();
        let now_text = format_time(now)?;
        let batch_id = Uuid::new_v4();
        let mut tx = self.pool.begin().await?;

        let rows = if let Some(requester_key_id) = requester_key_id {
            sqlx::query(
                r#"
                SELECT id
                FROM jobs
                WHERE requester_key_id = ? AND hidden_at IS NULL
                ORDER BY created_at DESC
                "#,
            )
            .bind(requester_key_id.to_string())
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id
                FROM jobs
                WHERE hidden_at IS NULL
                ORDER BY created_at DESC
                "#,
            )
            .fetch_all(&mut *tx)
            .await?
        };

        let job_ids = rows
            .into_iter()
            .map(|row| row.get::<String, _>("id"))
            .collect::<Vec<_>>();
        if job_ids.is_empty() {
            tx.commit().await?;
            return Ok(ClearJobsResponse {
                batch_id: None,
                hidden: 0,
                history_deleted: false,
            });
        }

        sqlx::query(
            r#"
            INSERT INTO job_hide_batches (
                id,
                actor_key_id,
                actor_label,
                hidden_count,
                restored_count,
                created_at,
                restored_at
            )
            VALUES (?, ?, ?, ?, 0, ?, NULL)
            "#,
        )
        .bind(batch_id.to_string())
        .bind(actor_key_id.map(|id| id.to_string()))
        .bind(actor_label)
        .bind(i64::try_from(job_ids.len()).unwrap_or(i64::MAX))
        .bind(&now_text)
        .execute(&mut *tx)
        .await?;

        for job_id in &job_ids {
            sqlx::query(
                r#"
                INSERT INTO job_hide_entries (
                    batch_id,
                    job_id,
                    created_at,
                    restored_at
                )
                VALUES (?, ?, ?, NULL)
                "#,
            )
            .bind(batch_id.to_string())
            .bind(job_id)
            .bind(&now_text)
            .execute(&mut *tx)
            .await?;
        }

        let result = if let Some(requester_key_id) = requester_key_id {
            sqlx::query(
                r#"
                UPDATE jobs
                SET hidden_at = ?,
                    updated_at = ?
                WHERE requester_key_id = ? AND hidden_at IS NULL
                "#,
            )
            .bind(&now_text)
            .bind(&now_text)
            .bind(requester_key_id.to_string())
            .execute(&mut *tx)
            .await?
        } else {
            sqlx::query(
                r#"
                UPDATE jobs
                SET hidden_at = ?,
                    updated_at = ?
                WHERE hidden_at IS NULL
                "#,
            )
            .bind(&now_text)
            .bind(&now_text)
            .execute(&mut *tx)
            .await?
        };

        tx.commit().await?;

        Ok(ClearJobsResponse {
            batch_id: Some(batch_id),
            hidden: result.rows_affected(),
            history_deleted: false,
        })
    }

    pub async fn list_hidden_job_batches(
        &self,
        actor_key_id: Option<Uuid>,
        limit: usize,
    ) -> Result<Vec<HiddenJobBatchView>> {
        let limit = limit.clamp(1, 200) as i64;
        let rows = if let Some(actor_key_id) = actor_key_id {
            sqlx::query(
                r#"
                SELECT id, actor_key_id, actor_label, hidden_count, restored_count, created_at, restored_at
                FROM job_hide_batches
                WHERE actor_key_id = ?
                ORDER BY created_at DESC
                LIMIT ?
                "#,
            )
            .bind(actor_key_id.to_string())
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id, actor_key_id, actor_label, hidden_count, restored_count, created_at, restored_at
                FROM job_hide_batches
                ORDER BY created_at DESC
                LIMIT ?
                "#,
            )
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        rows.into_iter().map(row_to_hidden_batch).collect()
    }

    pub async fn restore_latest_hidden_job_batch(
        &self,
        actor_key_id: Option<Uuid>,
    ) -> Result<RestoreJobsResponse> {
        let batch_id = self.latest_restorable_batch_id(actor_key_id).await?;
        if let Some(batch_id) = batch_id {
            self.restore_hidden_job_batch(actor_key_id, batch_id).await
        } else {
            Ok(RestoreJobsResponse {
                batch_id: None,
                restored: 0,
                history_deleted: false,
            })
        }
    }

    async fn latest_restorable_batch_id(&self, actor_key_id: Option<Uuid>) -> Result<Option<Uuid>> {
        let row = if let Some(actor_key_id) = actor_key_id {
            sqlx::query(
                r#"
                SELECT id
                FROM job_hide_batches
                WHERE actor_key_id = ? AND restored_at IS NULL
                ORDER BY created_at DESC
                LIMIT 1
                "#,
            )
            .bind(actor_key_id.to_string())
            .fetch_optional(&self.pool)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id
                FROM job_hide_batches
                WHERE restored_at IS NULL
                ORDER BY created_at DESC
                LIMIT 1
                "#,
            )
            .fetch_optional(&self.pool)
            .await?
        };

        row.map(|row| {
            let id: String = row.get("id");
            Uuid::parse_str(&id)
                .map_err(|error| RkError::BadRequest(format!("invalid hidden batch id: {error}")))
        })
        .transpose()
    }

    pub async fn restore_hidden_job_batch(
        &self,
        actor_key_id: Option<Uuid>,
        batch_id: Uuid,
    ) -> Result<RestoreJobsResponse> {
        let now_text = format_time(OffsetDateTime::now_utc())?;
        let mut tx = self.pool.begin().await?;

        let batch_row = if let Some(actor_key_id) = actor_key_id {
            sqlx::query(
                r#"
                SELECT id
                FROM job_hide_batches
                WHERE id = ? AND actor_key_id = ? AND restored_at IS NULL
                "#,
            )
            .bind(batch_id.to_string())
            .bind(actor_key_id.to_string())
            .fetch_optional(&mut *tx)
            .await?
        } else {
            sqlx::query(
                r#"
                SELECT id
                FROM job_hide_batches
                WHERE id = ? AND restored_at IS NULL
                "#,
            )
            .bind(batch_id.to_string())
            .fetch_optional(&mut *tx)
            .await?
        };

        if batch_row.is_none() {
            tx.commit().await?;
            return Ok(RestoreJobsResponse {
                batch_id: None,
                restored: 0,
                history_deleted: false,
            });
        }

        let rows = sqlx::query(
            r#"
            SELECT job_id
            FROM job_hide_entries
            WHERE batch_id = ? AND restored_at IS NULL
            "#,
        )
        .bind(batch_id.to_string())
        .fetch_all(&mut *tx)
        .await?;
        let job_ids = rows
            .into_iter()
            .map(|row| row.get::<String, _>("job_id"))
            .collect::<Vec<_>>();

        let mut restored = 0u64;
        for job_id in &job_ids {
            let result = sqlx::query(
                r#"
                UPDATE jobs
                SET hidden_at = NULL,
                    updated_at = ?
                WHERE id = ? AND hidden_at IS NOT NULL
                "#,
            )
            .bind(&now_text)
            .bind(job_id)
            .execute(&mut *tx)
            .await?;
            restored += result.rows_affected();
        }

        sqlx::query(
            r#"
            UPDATE job_hide_entries
            SET restored_at = ?
            WHERE batch_id = ? AND restored_at IS NULL
            "#,
        )
        .bind(&now_text)
        .bind(batch_id.to_string())
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            UPDATE job_hide_batches
            SET restored_at = ?,
                restored_count = ?
            WHERE id = ?
            "#,
        )
        .bind(&now_text)
        .bind(i64::try_from(restored).unwrap_or(i64::MAX))
        .bind(batch_id.to_string())
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(RestoreJobsResponse {
            batch_id: Some(batch_id),
            restored,
            history_deleted: false,
        })
    }

    pub async fn job_belongs_to_key(&self, id: Uuid, requester_key_id: Uuid) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM jobs
            WHERE id = ? AND requester_key_id = ?
            "#,
        )
        .bind(id.to_string())
        .bind(requester_key_id.to_string())
        .fetch_one(&self.pool)
        .await?;

        Ok(count > 0)
    }

    pub async fn find_api_key(&self, key: &str) -> Result<Option<ApiKeyRecord>> {
        let key_hash = hash_api_key(key);
        let Some(row) = sqlx::query(
            r#"
            SELECT id,
                   label,
                   key_hash,
                   key_prefix,
                   role,
                   max_download_bytes,
                   allow_browser_probe,
                   allow_ytdlp,
                   allow_external_adapters,
                   allow_login_profile,
                   created_at,
                   revoked_at
            FROM api_keys
            WHERE key_hash = ? AND revoked_at IS NULL
            "#,
        )
        .bind(key_hash)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(row_to_api_key(row)?))
    }

    pub async fn get_api_key(&self, id: Uuid) -> Result<Option<ApiKeyRecord>> {
        let Some(row) = sqlx::query(
            r#"
            SELECT id,
                   label,
                   key_hash,
                   key_prefix,
                   role,
                   max_download_bytes,
                   allow_browser_probe,
                   allow_ytdlp,
                   allow_external_adapters,
                   allow_login_profile,
                   created_at,
                   revoked_at
            FROM api_keys
            WHERE id = ?
            "#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(row_to_api_key(row)?))
    }

    pub async fn has_active_admin_key(&self) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"
            SELECT COUNT(*)
            FROM api_keys
            WHERE role = 'admin' AND revoked_at IS NULL
            "#,
        )
        .fetch_one(&self.pool)
        .await?;
        Ok(count > 0)
    }

    pub async fn list_api_keys(&self) -> Result<Vec<ApiKeyView>> {
        let rows = sqlx::query(
            r#"
            SELECT id,
                   label,
                   key_hash,
                   key_prefix,
                   role,
                   max_download_bytes,
                   allow_browser_probe,
                   allow_ytdlp,
                   allow_external_adapters,
                   allow_login_profile,
                   created_at,
                   revoked_at
            FROM api_keys
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(row_to_api_key)
            .map(|record| record.map(ApiKeyView::from))
            .collect()
    }

    pub async fn create_user_key(
        &self,
        request: CreateUserKeyRequest,
    ) -> Result<CreatedUserKeyResponse> {
        if let Some(value) = request.max_download_mb {
            validate_range("max_download_mb", value, 1, 102_400)?;
        }
        let key = request
            .key
            .as_deref()
            .map(normalize_custom_api_key)
            .transpose()?
            .unwrap_or_else(|| generate_api_key("rk_user"));
        let record = self
            .insert_api_key(ApiKeyRecord {
                id: Uuid::new_v4(),
                label: normalize_api_key_label(request.label.as_deref()),
                key_hash: hash_api_key(&key),
                key_prefix: key.chars().take(16).collect(),
                role: ApiKeyRole::User,
                max_download_bytes: request.max_download_mb.map(mib_to_bytes),
                allow_browser_probe: request.allow_browser_probe,
                allow_ytdlp: request.allow_ytdlp,
                allow_external_adapters: request.allow_external_adapters || request.allow_ytdlp,
                allow_login_profile: request.allow_login_profile,
                created_at: OffsetDateTime::now_utc(),
                revoked_at: None,
            })
            .await?;

        Ok(CreatedUserKeyResponse {
            key,
            record: record.into(),
        })
    }

    pub async fn ensure_admin_key_from_secret(&self, key: &str) -> Result<()> {
        let key = key.trim();
        if key.is_empty() || self.has_active_admin_key().await? {
            return Ok(());
        }

        self.insert_api_key(ApiKeyRecord {
            id: Uuid::new_v4(),
            label: "管理密钥".to_string(),
            key_hash: hash_api_key(key),
            key_prefix: key.chars().take(16).collect(),
            role: ApiKeyRole::Admin,
            max_download_bytes: None,
            allow_browser_probe: true,
            allow_ytdlp: true,
            allow_external_adapters: true,
            allow_login_profile: true,
            created_at: OffsetDateTime::now_utc(),
            revoked_at: None,
        })
        .await?;
        Ok(())
    }

    pub async fn rotate_admin_key(
        &self,
        custom_key: Option<&str>,
    ) -> Result<RotatedAdminKeyResponse> {
        let key = custom_key
            .map(normalize_custom_api_key)
            .transpose()?
            .unwrap_or_else(|| generate_api_key("rk_admin"));
        let now = OffsetDateTime::now_utc();
        let record = ApiKeyRecord {
            id: Uuid::new_v4(),
            label: "管理密钥".to_string(),
            key_hash: hash_api_key(&key),
            key_prefix: key.chars().take(16).collect(),
            role: ApiKeyRole::Admin,
            max_download_bytes: None,
            allow_browser_probe: true,
            allow_ytdlp: true,
            allow_external_adapters: true,
            allow_login_profile: true,
            created_at: now,
            revoked_at: None,
        };

        let mut tx = self.pool.begin().await?;
        sqlx::query(
            r#"
            UPDATE api_keys
            SET revoked_at = ?
            WHERE role = 'admin' AND revoked_at IS NULL
            "#,
        )
        .bind(format_time(now)?)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
            INSERT INTO api_keys (
                id,
                label,
                key_hash,
                key_prefix,
                role,
                max_download_bytes,
                allow_browser_probe,
                allow_ytdlp,
                allow_external_adapters,
                allow_login_profile,
                created_at,
                revoked_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(record.id.to_string())
        .bind(&record.label)
        .bind(&record.key_hash)
        .bind(&record.key_prefix)
        .bind(record.role.as_str())
        .bind(record.max_download_bytes.map(|value| value as i64))
        .bind(record.allow_browser_probe)
        .bind(record.allow_ytdlp)
        .bind(record.allow_external_adapters)
        .bind(record.allow_login_profile)
        .bind(format_time(record.created_at)?)
        .bind(record.revoked_at.map(format_time).transpose()?)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;

        Ok(RotatedAdminKeyResponse {
            key,
            record: record.into(),
        })
    }

    async fn insert_api_key(&self, record: ApiKeyRecord) -> Result<ApiKeyRecord> {
        sqlx::query(
            r#"
            INSERT INTO api_keys (
                id,
                label,
                key_hash,
                key_prefix,
                role,
                max_download_bytes,
                allow_browser_probe,
                allow_ytdlp,
                allow_external_adapters,
                allow_login_profile,
                created_at,
                revoked_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(record.id.to_string())
        .bind(&record.label)
        .bind(&record.key_hash)
        .bind(&record.key_prefix)
        .bind(record.role.as_str())
        .bind(record.max_download_bytes.map(|value| value as i64))
        .bind(record.allow_browser_probe)
        .bind(record.allow_ytdlp)
        .bind(record.allow_external_adapters)
        .bind(record.allow_login_profile)
        .bind(format_time(record.created_at)?)
        .bind(record.revoked_at.map(format_time).transpose()?)
        .execute(&self.pool)
        .await?;
        Ok(record)
    }

    pub async fn revoke_api_key(&self, id: Uuid) -> Result<bool> {
        let result = sqlx::query(
            r#"
            UPDATE api_keys
            SET revoked_at = ?
            WHERE id = ? AND revoked_at IS NULL
            "#,
        )
        .bind(format_time(OffsetDateTime::now_utc())?)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    pub async fn runtime_setting_values(&self) -> Result<HashMap<String, String>> {
        let rows = sqlx::query(
            r#"
            SELECT key, value
            FROM runtime_settings
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| (row.get::<String, _>("key"), row.get::<String, _>("value")))
            .collect())
    }

    pub async fn update_runtime_settings(
        &self,
        request: UpdateRuntimeSettingsRequest,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        if let Some(value) = request.public_base_url {
            let value = normalize_public_base_url(&value)?;
            upsert_runtime_setting(&mut tx, "public_base_url", &value).await?;
        }
        if let Some(value) = request.max_download_mb {
            validate_range("max_download_mb", value, 1, 102_400)?;
            upsert_runtime_setting(&mut tx, "max_download_mb", &value.to_string()).await?;
        }
        if let Some(value) = request.download_timeout_seconds {
            validate_range("download_timeout_seconds", value, 5, 86_400)?;
            upsert_runtime_setting(&mut tx, "download_timeout_seconds", &value.to_string()).await?;
        }
        if let Some(value) = request.yt_dlp_timeout_seconds {
            validate_range("yt_dlp_timeout_seconds", value, 5, 86_400)?;
            upsert_runtime_setting(&mut tx, "yt_dlp_timeout_seconds", &value.to_string()).await?;
        }
        if let Some(value) = request.yt_dlp_max_json_mb {
            validate_range("yt_dlp_max_json_mb", value as u64, 1, 256)?;
            upsert_runtime_setting(&mut tx, "yt_dlp_max_json_mb", &value.to_string()).await?;
        }
        if let Some(value) = request.job_ttl_hours {
            validate_range("job_ttl_hours", value, 1, 8_760)?;
            upsert_runtime_setting(&mut tx, "job_ttl_hours", &value.to_string()).await?;
        }
        if let Some(value) = request.page_archive_max_resources {
            validate_range("page_archive_max_resources", value as u64, 1, 2_000)?;
            upsert_runtime_setting(&mut tx, "page_archive_max_resources", &value.to_string())
                .await?;
        }
        if let Some(value) = request.page_archive_max_resource_mb {
            validate_range("page_archive_max_resource_mb", value, 1, 1_024)?;
            upsert_runtime_setting(&mut tx, "page_archive_max_resource_mb", &value.to_string())
                .await?;
        }
        if let Some(value) = request.page_archive_max_total_mb {
            validate_range("page_archive_max_total_mb", value, 1, 4_000)?;
            upsert_runtime_setting(&mut tx, "page_archive_max_total_mb", &value.to_string())
                .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn set_selected_candidates(&self, id: Uuid, candidate_ids: &[Uuid]) -> Result<()> {
        let values = candidate_ids
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>();
        sqlx::query(
            r#"
            UPDATE jobs
            SET selected_candidate_ids_json = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(serde_json::to_string(&values)?)
        .bind(format_time(OffsetDateTime::now_utc())?)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn reset_for_profile_resume(&self, id: Uuid, profile_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = ?,
                media_url = NULL,
                error = NULL,
                error_class = 'none',
                discovery = ?,
                auth_mode = ?,
                profile_id = ?,
                selected_candidate_ids_json = '[]',
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(JobStatus::Queued.as_str())
        .bind(DiscoveryMode::Browser.as_str())
        .bind(AuthMode::Profile.as_str())
        .bind(profile_id)
        .bind(format_time(OffsetDateTime::now_utc())?)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        sqlx::query("DELETE FROM source_candidates WHERE job_id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn reset_for_page_archive_force(&self, id: Uuid) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = ?,
                media_url = NULL,
                error = NULL,
                error_class = 'none',
                discovery = ?,
                auth_mode = ?,
                outputs_json = ?,
                selected_candidate_ids_json = '[]',
                completed_at = NULL,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(JobStatus::Queued.as_str())
        .bind(DiscoveryMode::Browser.as_str())
        .bind(AuthMode::None.as_str())
        .bind(serde_json::to_string(&[OutputKind::PageHtml.as_str()])?)
        .bind(format_time(OffsetDateTime::now_utc())?)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        sqlx::query("DELETE FROM source_candidates WHERE job_id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        sqlx::query("DELETE FROM artifacts WHERE job_id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    pub async fn attach_profile_for_job(&self, id: Uuid, profile_id: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET profile_id = ?,
                auth_mode = ?,
                discovery = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(profile_id)
        .bind(AuthMode::Profile.as_str())
        .bind(DiscoveryMode::Browser.as_str())
        .bind(format_time(OffsetDateTime::now_utc())?)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn replace_candidates(
        &self,
        job_id: uuid::Uuid,
        candidates: &[MediaCandidate],
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM source_candidates WHERE job_id = ?")
            .bind(job_id.to_string())
            .execute(&mut *tx)
            .await?;

        for candidate in candidates {
            sqlx::query(
                r#"
                INSERT INTO source_candidates (
                    id,
                    job_id,
                    url,
                    kind,
                    extractor,
                    method,
                    status,
                    content_type,
                    content_length,
                    resource_type,
                    initiator_url,
                    quality_label,
                    score,
                    requires_authorization,
                    platform,
                    route,
                    extractor_confidence,
                    protection,
                    requires_profile,
                    ttl_hint_seconds,
                    ad_risk,
                    evidence_count,
                    paired_candidate_ids_json,
                    failure_reason,
                    validation_state,
                    metadata_json,
                    created_at,
                    score_breakdown_json,
                    selected,
                    selection_reason,
                    validation_status,
                    resolved_ip,
                    final_url_after_redirects,
                    expires_at,
                    discovered_by_event_id
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(candidate.id.to_string())
            .bind(candidate.job_id.to_string())
            .bind(&candidate.url)
            .bind(candidate.kind.as_str())
            .bind(&candidate.extractor)
            .bind(&candidate.method)
            .bind(candidate.status.map(i64::from))
            .bind(&candidate.content_type)
            .bind(candidate.content_length)
            .bind(&candidate.resource_type)
            .bind(&candidate.initiator_url)
            .bind(&candidate.quality_label)
            .bind(candidate.score)
            .bind(candidate.requires_authorization)
            .bind(candidate.platform.map(|platform| platform.as_str().to_string()))
            .bind(&candidate.route)
            .bind(candidate.extractor_confidence)
            .bind(candidate.protection.map(|protection| protection.as_str().to_string()))
            .bind(candidate.requires_profile)
            .bind(candidate.ttl_hint_seconds)
            .bind(candidate.ad_risk)
            .bind(candidate.evidence_count)
            .bind(serde_json::to_string(&candidate.paired_candidate_ids)?)
            .bind(&candidate.failure_reason)
            .bind(candidate.validation_state.map(|state| state.as_str().to_string()))
            .bind(candidate.metadata_json.to_string())
            .bind(format_time(candidate.created_at)?)
            .bind(candidate.score_breakdown_json.to_string())
            .bind(candidate.selected)
            .bind(&candidate.selection_reason)
            .bind(&candidate.validation_status)
            .bind(&candidate.resolved_ip)
            .bind(&candidate.final_url_after_redirects)
            .bind(candidate.expires_at.map(format_time).transpose()?)
            .bind(candidate.discovered_by_event_id.map(|id| id.to_string()))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn list_candidates(&self, job_id: uuid::Uuid) -> Result<Vec<MediaCandidate>> {
        let rows = sqlx::query(
            r#"
            SELECT id,
                   job_id,
                   url,
                   kind,
                   extractor,
                   method,
                   status,
                   content_type,
                   content_length,
                   resource_type,
                   initiator_url,
                   quality_label,
                   score,
                   requires_authorization,
                   platform,
                   route,
                   extractor_confidence,
                   protection,
                   requires_profile,
                   ttl_hint_seconds,
                   ad_risk,
                   evidence_count,
                   paired_candidate_ids_json,
                   failure_reason,
                   validation_state,
                   metadata_json,
                   created_at,
                   score_breakdown_json,
                   selected,
                   selection_reason,
                   validation_status,
                   resolved_ip,
                   final_url_after_redirects,
                   expires_at,
                   discovered_by_event_id
            FROM source_candidates
            WHERE job_id = ?
            ORDER BY score DESC, created_at ASC
            "#,
        )
        .bind(job_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_candidate).collect()
    }

    pub async fn get_candidate(
        &self,
        job_id: uuid::Uuid,
        candidate_id: uuid::Uuid,
    ) -> Result<Option<MediaCandidate>> {
        let Some(row) = sqlx::query(
            r#"
            SELECT id,
                   job_id,
                   url,
                   kind,
                   extractor,
                   method,
                   status,
                   content_type,
                   content_length,
                   resource_type,
                   initiator_url,
                   quality_label,
                   score,
                   requires_authorization,
                   platform,
                   route,
                   extractor_confidence,
                   protection,
                   requires_profile,
                   ttl_hint_seconds,
                   ad_risk,
                   evidence_count,
                   paired_candidate_ids_json,
                   failure_reason,
                   validation_state,
                   metadata_json,
                   created_at,
                   score_breakdown_json,
                   selected,
                   selection_reason,
                   validation_status,
                   resolved_ip,
                   final_url_after_redirects,
                   expires_at,
                   discovered_by_event_id
            FROM source_candidates
            WHERE job_id = ? AND id = ?
            "#,
        )
        .bind(job_id.to_string())
        .bind(candidate_id.to_string())
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        Ok(Some(row_to_candidate(row)?))
    }

    pub async fn insert_artifact(&self, artifact: &ArtifactView) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO artifacts (
                id,
                job_id,
                kind,
                path,
                media_url,
                content_type,
                bytes,
                created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(artifact.id.to_string())
        .bind(artifact.job_id.to_string())
        .bind(artifact.kind.as_str())
        .bind(&artifact.path)
        .bind(&artifact.media_url)
        .bind(&artifact.content_type)
        .bind(artifact.bytes)
        .bind(format_time(artifact.created_at)?)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn list_artifacts(&self, job_id: uuid::Uuid) -> Result<Vec<ArtifactView>> {
        let rows = sqlx::query(
            r#"
            SELECT id,
                   job_id,
                   kind,
                   path,
                   media_url,
                   content_type,
                   bytes,
                   created_at
            FROM artifacts
            WHERE job_id = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(job_id.to_string())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_artifact).collect()
    }

    pub async fn update_status(&self, id: Uuid, status: JobStatus) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = ?,
                updated_at = ?,
                error = NULL
            WHERE id = ?
            "#,
        )
        .bind(status.as_str())
        .bind(format_time(OffsetDateTime::now_utc())?)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_ready(&self, id: Uuid, media_url: &str) -> Result<()> {
        let now = format_time(OffsetDateTime::now_utc())?;
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = ?,
                media_url = ?,
                error = NULL,
                error_class = 'none',
                updated_at = ?,
                completed_at = ?
            WHERE id = ?
            "#,
        )
        .bind(JobStatus::Ready.as_str())
        .bind(media_url)
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_error(&self, id: Uuid, error: &str, error_class: ErrorClass) -> Result<()> {
        let now = format_time(OffsetDateTime::now_utc())?;
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = ?,
                error = ?,
                error_class = ?,
                updated_at = ?,
                completed_at = ?
            WHERE id = ?
            "#,
        )
        .bind(JobStatus::Error.as_str())
        .bind(error)
        .bind(error_class.as_str())
        .bind(&now)
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_needs_profile(
        &self,
        id: Uuid,
        error: &str,
        error_class: ErrorClass,
    ) -> Result<()> {
        let now = format_time(OffsetDateTime::now_utc())?;
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = ?,
                error = ?,
                error_class = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(JobStatus::NeedsProfile.as_str())
        .bind(error)
        .bind(error_class.as_str())
        .bind(&now)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Set `started_at` to now the first time a job begins processing.
    pub async fn mark_started(&self, id: Uuid) -> Result<()> {
        sqlx::query(
            "UPDATE jobs SET started_at = COALESCE(started_at, ?), updated_at = ? WHERE id = ?",
        )
        .bind(format_time(OffsetDateTime::now_utc())?)
        .bind(format_time(OffsetDateTime::now_utc())?)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Increment the attempt counter and return the new value.
    pub async fn increment_attempt(&self, id: Uuid) -> Result<i64> {
        sqlx::query("UPDATE jobs SET attempt_count = attempt_count + 1 WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        let row = sqlx::query("SELECT attempt_count FROM jobs WHERE id = ?")
            .bind(id.to_string())
            .fetch_one(&self.pool)
            .await?;
        Ok(row.get::<i64, _>("attempt_count"))
    }

    /// Record which extractor chain produced the result, e.g.
    /// "street_voice>browser_probe".
    pub async fn set_resolved_extractor(&self, id: Uuid, chain: &str) -> Result<()> {
        sqlx::query("UPDATE jobs SET resolved_extractor = ? WHERE id = ?")
            .bind(chain)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Persist a candidate selection outcome (auditability: "we chose X because").
    pub async fn set_candidate_selection(
        &self,
        candidate_id: Uuid,
        selected: bool,
        reason: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE source_candidates SET selected = ?, selection_reason = ? WHERE id = ?")
            .bind(selected)
            .bind(reason)
            .bind(candidate_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn set_candidate_validation_status(
        &self,
        candidate_id: Uuid,
        status: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE source_candidates SET validation_status = ? WHERE id = ?")
            .bind(status)
            .bind(candidate_id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // --- Observability writers -------------------------------------------------

    /// Persist one outbound request record. Headers are expected to be redacted
    /// by the caller before reaching this point.
    pub async fn log_request(&self, log: &RequestLog) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO request_log (
                id, job_id, candidate_id, phase, method, url, host, resolved_ip, egress_ip,
                request_headers_json, user_agent, referer, profile_id, response_status,
                response_headers_json, content_type, content_length, bytes_read,
                redirect_chain_json, http_version, tls_version, started_at, ended_at,
                duration_ms, error_class, error_message
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(log.id.to_string())
        .bind(log.job_id.to_string())
        .bind(log.candidate_id.map(|id| id.to_string()))
        .bind(log.phase.as_str())
        .bind(&log.method)
        .bind(&log.url)
        .bind(&log.host)
        .bind(&log.resolved_ip)
        .bind(&log.egress_ip)
        .bind(log.request_headers_json.to_string())
        .bind(&log.user_agent)
        .bind(&log.referer)
        .bind(&log.profile_id)
        .bind(log.response_status.map(i64::from))
        .bind(log.response_headers_json.as_ref().map(|v| v.to_string()))
        .bind(&log.content_type)
        .bind(log.content_length)
        .bind(log.bytes_read)
        .bind(log.redirect_chain_json.as_ref().map(|v| v.to_string()))
        .bind(&log.http_version)
        .bind(&log.tls_version)
        .bind(format_time(log.started_at)?)
        .bind(log.ended_at.map(format_time).transpose()?)
        .bind(log.duration_ms)
        .bind(log.error_class.as_str())
        .bind(&log.error_message)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Append a pipeline event, auto-assigning the next per-job sequence number.
    pub async fn log_event(&self, event: &PipelineEvent) -> Result<()> {
        let seq_row = sqlx::query(
            "SELECT COALESCE(MAX(seq), 0) + 1 AS next_seq FROM pipeline_events WHERE job_id = ?",
        )
        .bind(event.job_id.to_string())
        .fetch_one(&self.pool)
        .await?;
        let seq: i64 = seq_row.get("next_seq");

        sqlx::query(
            r#"
            INSERT INTO pipeline_events (
                id, job_id, seq, stage, actor, event_type, detail_json,
                candidate_id, request_log_id, created_at, duration_ms
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(event.id.to_string())
        .bind(event.job_id.to_string())
        .bind(seq)
        .bind(&event.stage)
        .bind(&event.actor)
        .bind(event.event_type.as_str())
        .bind(event.detail_json.to_string())
        .bind(event.candidate_id.map(|id| id.to_string()))
        .bind(event.request_log_id.map(|id| id.to_string()))
        .bind(format_time(event.created_at)?)
        .bind(event.duration_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_browser_session(&self, session: &BrowserSession) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO browser_sessions (
                id, job_id, profile_id, user_agent, viewport, locale, timezone, headed,
                final_url, page_title, event_count, candidate_count, playback_triggered,
                timed_out, warnings_json, console_errors_json, screenshot_path,
                started_at, ended_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(session.id.to_string())
        .bind(session.job_id.to_string())
        .bind(&session.profile_id)
        .bind(&session.user_agent)
        .bind(&session.viewport)
        .bind(&session.locale)
        .bind(&session.timezone)
        .bind(session.headed)
        .bind(&session.final_url)
        .bind(&session.page_title)
        .bind(session.event_count)
        .bind(session.candidate_count)
        .bind(session.playback_triggered)
        .bind(session.timed_out)
        .bind(session.warnings_json.to_string())
        .bind(session.console_errors_json.to_string())
        .bind(&session.screenshot_path)
        .bind(format_time(session.started_at)?)
        .bind(session.ended_at.map(format_time).transpose()?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_probe(&self, probe: &MediaProbe) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO media_probes (
                id, job_id, candidate_id, container, duration_s, overall_bitrate,
                streams_json, raw_json, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(probe.id.to_string())
        .bind(probe.job_id.to_string())
        .bind(probe.candidate_id.map(|id| id.to_string()))
        .bind(&probe.container)
        .bind(probe.duration_s)
        .bind(probe.overall_bitrate)
        .bind(probe.streams_json.to_string())
        .bind(probe.raw_json.to_string())
        .bind(format_time(probe.created_at)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn record_transcode(&self, run: &TranscodeRun) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO transcode_runs (
                id, job_id, candidate_id, tool, command_redacted, input_bytes, output_bytes,
                output_path, output_kind, profile, exit_code, stderr_tail, duration_ms, created_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(run.id.to_string())
        .bind(run.job_id.to_string())
        .bind(run.candidate_id.map(|id| id.to_string()))
        .bind(&run.tool)
        .bind(&run.command_redacted)
        .bind(run.input_bytes)
        .bind(run.output_bytes)
        .bind(&run.output_path)
        .bind(&run.output_kind)
        .bind(&run.profile)
        .bind(run.exit_code)
        .bind(&run.stderr_tail)
        .bind(run.duration_ms)
        .bind(format_time(run.created_at)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_domain_policy(&self, host: &str) -> Result<Option<DomainPolicy>> {
        let Some(row) = sqlx::query(
            r#"
            SELECT host, allow_mode, max_concurrency, crawl_delay_ms, requires_user_auth,
                   last_status, blocked_count, learned_api_pattern, notes, updated_at
            FROM domain_policies WHERE host = ?
            "#,
        )
        .bind(host)
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };
        Ok(Some(row_to_domain_policy(row)?))
    }

    pub async fn upsert_domain_policy(&self, policy: &DomainPolicy) -> Result<()> {
        sqlx::query(
            r#"
            INSERT INTO domain_policies (
                host, allow_mode, max_concurrency, crawl_delay_ms, requires_user_auth,
                last_status, blocked_count, learned_api_pattern, notes, updated_at
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(host) DO UPDATE SET
                allow_mode = excluded.allow_mode,
                max_concurrency = excluded.max_concurrency,
                crawl_delay_ms = excluded.crawl_delay_ms,
                requires_user_auth = excluded.requires_user_auth,
                last_status = excluded.last_status,
                blocked_count = excluded.blocked_count,
                learned_api_pattern = COALESCE(excluded.learned_api_pattern, domain_policies.learned_api_pattern),
                notes = excluded.notes,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&policy.host)
        .bind(&policy.allow_mode)
        .bind(policy.max_concurrency)
        .bind(policy.crawl_delay_ms)
        .bind(policy.requires_user_auth)
        .bind(policy.last_status)
        .bind(policy.blocked_count)
        .bind(&policy.learned_api_pattern)
        .bind(&policy.notes)
        .bind(format_time(policy.updated_at)?)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // --- Observability readers (job trace) -------------------------------------

    /// Aggregate the full timeline for a job: every step, request, browser
    /// session, ffprobe, and ffmpeg run.
    pub async fn get_trace(&self, job_id: Uuid) -> Result<JobTrace> {
        Ok(JobTrace {
            job_id,
            events: self.list_events(job_id).await?,
            requests: self.list_requests(job_id).await?,
            browser_sessions: self.list_browser_sessions(job_id).await?,
            probes: self.list_probes(job_id).await?,
            transcodes: self.list_transcodes(job_id).await?,
        })
    }

    async fn list_events(&self, job_id: Uuid) -> Result<Vec<PipelineEvent>> {
        let rows = sqlx::query(
            r#"
            SELECT id, job_id, seq, stage, actor, event_type, detail_json,
                   candidate_id, request_log_id, created_at, duration_ms
            FROM pipeline_events WHERE job_id = ? ORDER BY seq ASC
            "#,
        )
        .bind(job_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_event).collect()
    }

    async fn list_requests(&self, job_id: Uuid) -> Result<Vec<RequestLog>> {
        let rows = sqlx::query(
            r#"
            SELECT id, job_id, candidate_id, phase, method, url, host, resolved_ip, egress_ip,
                   request_headers_json, user_agent, referer, profile_id, response_status,
                   response_headers_json, content_type, content_length, bytes_read,
                   redirect_chain_json, http_version, tls_version, started_at, ended_at,
                   duration_ms, error_class, error_message
            FROM request_log WHERE job_id = ? ORDER BY started_at ASC
            "#,
        )
        .bind(job_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_request).collect()
    }

    async fn list_browser_sessions(&self, job_id: Uuid) -> Result<Vec<BrowserSession>> {
        let rows = sqlx::query(
            r#"
            SELECT id, job_id, profile_id, user_agent, viewport, locale, timezone, headed,
                   final_url, page_title, event_count, candidate_count, playback_triggered,
                   timed_out, warnings_json, console_errors_json, screenshot_path,
                   started_at, ended_at
            FROM browser_sessions WHERE job_id = ? ORDER BY started_at ASC
            "#,
        )
        .bind(job_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_browser_session).collect()
    }

    async fn list_probes(&self, job_id: Uuid) -> Result<Vec<MediaProbe>> {
        let rows = sqlx::query(
            r#"
            SELECT id, job_id, candidate_id, container, duration_s, overall_bitrate,
                   streams_json, raw_json, created_at
            FROM media_probes WHERE job_id = ? ORDER BY created_at ASC
            "#,
        )
        .bind(job_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_probe).collect()
    }

    async fn list_transcodes(&self, job_id: Uuid) -> Result<Vec<TranscodeRun>> {
        let rows = sqlx::query(
            r#"
            SELECT id, job_id, candidate_id, tool, command_redacted, input_bytes, output_bytes,
                   output_path, output_kind, profile, exit_code, stderr_tail, duration_ms, created_at
            FROM transcode_runs WHERE job_id = ? ORDER BY created_at ASC
            "#,
        )
        .bind(job_id.to_string())
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(row_to_transcode).collect()
    }

    pub async fn recover_pending(&self) -> Result<Vec<Uuid>> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = ?,
                updated_at = ?
            WHERE status IN (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(JobStatus::Queued.as_str())
        .bind(format_time(OffsetDateTime::now_utc())?)
        .bind(JobStatus::Queued.as_str())
        .bind(JobStatus::Resolving.as_str())
        .bind(JobStatus::CandidateSelected.as_str())
        .bind(JobStatus::Downloading.as_str())
        .bind(JobStatus::Capturing.as_str())
        .bind(JobStatus::Probing.as_str())
        .bind(JobStatus::Transcoding.as_str())
        .bind(JobStatus::Remuxing.as_str())
        .execute(&self.pool)
        .await?;

        let rows = sqlx::query(
            r#"
            SELECT id
            FROM jobs
            WHERE status = ?
            ORDER BY created_at ASC
            "#,
        )
        .bind(JobStatus::Queued.as_str())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let id: String = row.get("id");
                Uuid::parse_str(&id)
                    .map_err(|error| RkError::BadRequest(format!("invalid stored job id: {error}")))
            })
            .collect()
    }

    pub async fn expired_job_ids(&self, ttl_hours: u64) -> Result<Vec<Uuid>> {
        let cutoff = OffsetDateTime::now_utc() - time::Duration::hours(ttl_hours.max(1) as i64);
        let rows = sqlx::query(
            r#"
            SELECT id
            FROM jobs
            WHERE hidden_at IS NULL
              AND created_at < ?
              AND status IN (?, ?, ?, ?)
            "#,
        )
        .bind(format_time(cutoff)?)
        .bind(JobStatus::Ready.as_str())
        .bind(JobStatus::Error.as_str())
        .bind(JobStatus::CandidatesReady.as_str())
        .bind(JobStatus::NeedsProfile.as_str())
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let id: String = row.get("id");
                Uuid::parse_str(&id)
                    .map_err(|error| RkError::BadRequest(format!("invalid stored job id: {error}")))
            })
            .collect()
    }

    pub async fn hide_expired_jobs(&self, ids: &[Uuid]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let now = format_time(OffsetDateTime::now_utc())?;
        let mut hidden = 0u64;
        for id in ids {
            let result = sqlx::query(
                r#"
                UPDATE jobs
                SET hidden_at = COALESCE(hidden_at, ?),
                    updated_at = ?
                WHERE id = ?
                "#,
            )
            .bind(&now)
            .bind(&now)
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
            hidden = hidden.saturating_add(result.rows_affected());
        }
        Ok(hidden)
    }

    async fn migrate(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY NOT NULL,
                status TEXT NOT NULL,
                source_url TEXT NOT NULL,
                bitrate TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                status_url TEXT NOT NULL,
                media_url TEXT,
                error TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        self.add_column_if_missing("jobs", "discovery", "TEXT NOT NULL DEFAULT 'direct'")
            .await?;
        self.add_column_if_missing("jobs", "platform_hint", "TEXT NOT NULL DEFAULT 'auto'")
            .await?;
        self.add_column_if_missing(
            "jobs",
            "outputs_json",
            "TEXT NOT NULL DEFAULT '[\"audio\"]'",
        )
        .await?;
        self.add_column_if_missing(
            "jobs",
            "profile_id",
            "TEXT NOT NULL DEFAULT 'admin_default'",
        )
        .await?;
        self.add_column_if_missing("jobs", "auth_mode", "TEXT NOT NULL DEFAULT 'none'")
            .await?;
        self.add_column_if_missing(
            "jobs",
            "selected_candidate_ids_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS api_keys (
                id TEXT PRIMARY KEY NOT NULL,
                label TEXT NOT NULL,
                key_hash TEXT NOT NULL UNIQUE,
                key_prefix TEXT NOT NULL,
                role TEXT NOT NULL,
                max_download_bytes INTEGER,
                allow_browser_probe INTEGER NOT NULL,
                allow_ytdlp INTEGER NOT NULL,
                allow_external_adapters INTEGER NOT NULL DEFAULT 1,
                allow_login_profile INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                revoked_at TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        self.add_column_if_missing("api_keys", "max_download_bytes", "INTEGER")
            .await?;
        self.add_column_if_missing(
            "api_keys",
            "allow_external_adapters",
            "INTEGER NOT NULL DEFAULT 1",
        )
        .await?;
        self.add_column_if_missing(
            "api_keys",
            "allow_login_profile",
            "INTEGER NOT NULL DEFAULT 0",
        )
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_api_keys_role_created_at ON api_keys(role, created_at)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS runtime_settings (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Observability: requester provenance, resolved extractor chain, error
        // classification, attempt counter, and lifecycle timestamps.
        self.add_column_if_missing("jobs", "requester_ip", "TEXT")
            .await?;
        self.add_column_if_missing("jobs", "requester_user_agent", "TEXT")
            .await?;
        self.add_column_if_missing("jobs", "requester_label", "TEXT")
            .await?;
        self.add_column_if_missing("jobs", "requester_key_id", "TEXT")
            .await?;
        self.add_column_if_missing("jobs", "resolved_extractor", "TEXT")
            .await?;
        self.add_column_if_missing("jobs", "error_class", "TEXT NOT NULL DEFAULT 'none'")
            .await?;
        self.add_column_if_missing("jobs", "attempt_count", "INTEGER NOT NULL DEFAULT 0")
            .await?;
        self.add_column_if_missing("jobs", "started_at", "TEXT")
            .await?;
        self.add_column_if_missing("jobs", "completed_at", "TEXT")
            .await?;
        self.add_column_if_missing("jobs", "hidden_at", "TEXT")
            .await?;

        self.normalize_legacy_job_statuses().await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_jobs_status_created_at ON jobs(status, created_at)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_jobs_hidden_created_at ON jobs(hidden_at, created_at)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS job_hide_batches (
                id TEXT PRIMARY KEY NOT NULL,
                actor_key_id TEXT,
                actor_label TEXT,
                hidden_count INTEGER NOT NULL,
                restored_count INTEGER NOT NULL,
                created_at TEXT NOT NULL,
                restored_at TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_job_hide_batches_actor_created ON job_hide_batches(actor_key_id, created_at)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS job_hide_entries (
                batch_id TEXT NOT NULL,
                job_id TEXT NOT NULL,
                created_at TEXT NOT NULL,
                restored_at TEXT,
                PRIMARY KEY (batch_id, job_id)
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_job_hide_entries_job ON job_hide_entries(job_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS source_candidates (
                id TEXT PRIMARY KEY NOT NULL,
                job_id TEXT NOT NULL,
                url TEXT NOT NULL,
                kind TEXT NOT NULL,
                extractor TEXT NOT NULL,
                method TEXT NOT NULL,
                status INTEGER,
                content_type TEXT,
                content_length INTEGER,
                resource_type TEXT,
                initiator_url TEXT,
                quality_label TEXT,
                score INTEGER NOT NULL,
                requires_authorization INTEGER NOT NULL,
                platform TEXT,
                route TEXT,
                extractor_confidence INTEGER,
                protection TEXT,
                requires_profile INTEGER NOT NULL DEFAULT 0,
                ttl_hint_seconds INTEGER,
                ad_risk INTEGER NOT NULL DEFAULT 0,
                evidence_count INTEGER NOT NULL DEFAULT 1,
                paired_candidate_ids_json TEXT NOT NULL DEFAULT '[]',
                failure_reason TEXT,
                validation_state TEXT,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        // Observability columns on candidates: scoring breakdown, selection
        // outcome, pre-capture validation, resolved IP, redirect target, and a
        // parsed signed-URL expiry.
        self.add_column_if_missing(
            "source_candidates",
            "score_breakdown_json",
            "TEXT NOT NULL DEFAULT '{}'",
        )
        .await?;
        self.add_column_if_missing(
            "source_candidates",
            "selected",
            "INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        self.add_column_if_missing("source_candidates", "selection_reason", "TEXT")
            .await?;
        self.add_column_if_missing("source_candidates", "validation_status", "TEXT")
            .await?;
        self.add_column_if_missing("source_candidates", "resolved_ip", "TEXT")
            .await?;
        self.add_column_if_missing("source_candidates", "final_url_after_redirects", "TEXT")
            .await?;
        self.add_column_if_missing("source_candidates", "expires_at", "TEXT")
            .await?;
        self.add_column_if_missing("source_candidates", "discovered_by_event_id", "TEXT")
            .await?;
        self.add_column_if_missing("source_candidates", "platform", "TEXT")
            .await?;
        self.add_column_if_missing("source_candidates", "route", "TEXT")
            .await?;
        self.add_column_if_missing("source_candidates", "extractor_confidence", "INTEGER")
            .await?;
        self.add_column_if_missing("source_candidates", "protection", "TEXT")
            .await?;
        self.add_column_if_missing(
            "source_candidates",
            "requires_profile",
            "INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        self.add_column_if_missing("source_candidates", "ttl_hint_seconds", "INTEGER")
            .await?;
        self.add_column_if_missing("source_candidates", "ad_risk", "INTEGER NOT NULL DEFAULT 0")
            .await?;
        self.add_column_if_missing(
            "source_candidates",
            "evidence_count",
            "INTEGER NOT NULL DEFAULT 1",
        )
        .await?;
        self.add_column_if_missing(
            "source_candidates",
            "paired_candidate_ids_json",
            "TEXT NOT NULL DEFAULT '[]'",
        )
        .await?;
        self.add_column_if_missing("source_candidates", "failure_reason", "TEXT")
            .await?;
        self.add_column_if_missing("source_candidates", "validation_state", "TEXT")
            .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_source_candidates_job_score ON source_candidates(job_id, score)",
        )
        .execute(&self.pool)
        .await?;

        self.migrate_observability().await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS artifacts (
                id TEXT PRIMARY KEY NOT NULL,
                job_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                path TEXT NOT NULL,
                media_url TEXT NOT NULL,
                content_type TEXT NOT NULL,
                bytes INTEGER NOT NULL,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_artifacts_job ON artifacts(job_id)")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Create the append-only observability tables: every outbound request,
    /// every pipeline step, browser sessions, ffprobe results, ffmpeg runs, and
    /// per-host policy. All secrets are redacted before they reach these tables.
    async fn migrate_observability(&self) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS request_log (
                id TEXT PRIMARY KEY NOT NULL,
                job_id TEXT NOT NULL,
                candidate_id TEXT,
                phase TEXT NOT NULL,
                method TEXT NOT NULL,
                url TEXT NOT NULL,
                host TEXT,
                resolved_ip TEXT,
                egress_ip TEXT,
                request_headers_json TEXT NOT NULL,
                user_agent TEXT,
                referer TEXT,
                profile_id TEXT,
                response_status INTEGER,
                response_headers_json TEXT,
                content_type TEXT,
                content_length INTEGER,
                bytes_read INTEGER,
                redirect_chain_json TEXT,
                http_version TEXT,
                tls_version TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_ms INTEGER,
                error_class TEXT NOT NULL,
                error_message TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_request_log_job ON request_log(job_id, started_at)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS pipeline_events (
                id TEXT PRIMARY KEY NOT NULL,
                job_id TEXT NOT NULL,
                seq INTEGER NOT NULL,
                stage TEXT NOT NULL,
                actor TEXT NOT NULL,
                event_type TEXT NOT NULL,
                detail_json TEXT NOT NULL,
                candidate_id TEXT,
                request_log_id TEXT,
                created_at TEXT NOT NULL,
                duration_ms INTEGER
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_pipeline_events_job_seq ON pipeline_events(job_id, seq)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS browser_sessions (
                id TEXT PRIMARY KEY NOT NULL,
                job_id TEXT NOT NULL,
                profile_id TEXT NOT NULL,
                user_agent TEXT,
                viewport TEXT,
                locale TEXT,
                timezone TEXT,
                headed INTEGER NOT NULL,
                final_url TEXT,
                page_title TEXT,
                event_count INTEGER NOT NULL,
                candidate_count INTEGER NOT NULL,
                playback_triggered INTEGER NOT NULL,
                timed_out INTEGER NOT NULL,
                warnings_json TEXT NOT NULL,
                console_errors_json TEXT NOT NULL,
                screenshot_path TEXT,
                started_at TEXT NOT NULL,
                ended_at TEXT
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_browser_sessions_job ON browser_sessions(job_id)",
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS media_probes (
                id TEXT PRIMARY KEY NOT NULL,
                job_id TEXT NOT NULL,
                candidate_id TEXT,
                container TEXT,
                duration_s REAL,
                overall_bitrate INTEGER,
                streams_json TEXT NOT NULL,
                raw_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_media_probes_job ON media_probes(job_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS transcode_runs (
                id TEXT PRIMARY KEY NOT NULL,
                job_id TEXT NOT NULL,
                candidate_id TEXT,
                tool TEXT NOT NULL,
                command_redacted TEXT NOT NULL,
                input_bytes INTEGER,
                output_bytes INTEGER,
                output_path TEXT,
                output_kind TEXT,
                profile TEXT,
                exit_code INTEGER,
                stderr_tail TEXT,
                duration_ms INTEGER,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_transcode_runs_job ON transcode_runs(job_id)")
            .execute(&self.pool)
            .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS domain_policies (
                host TEXT PRIMARY KEY NOT NULL,
                allow_mode TEXT NOT NULL,
                max_concurrency INTEGER NOT NULL,
                crawl_delay_ms INTEGER NOT NULL,
                requires_user_auth INTEGER NOT NULL,
                last_status INTEGER,
                blocked_count INTEGER NOT NULL,
                learned_api_pattern TEXT,
                notes TEXT,
                updated_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn normalize_legacy_job_statuses(&self) -> Result<()> {
        sqlx::query("UPDATE jobs SET status = ? WHERE status = ?")
            .bind(JobStatus::Error.as_str())
            .bind("failed")
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn add_column_if_missing(
        &self,
        table: &str,
        column: &str,
        definition: &str,
    ) -> Result<()> {
        let pragma = format!("PRAGMA table_info({table})");
        let rows = sqlx::query(&pragma).fetch_all(&self.pool).await?;
        let exists = rows.iter().any(|row| {
            let name: String = row.get("name");
            name == column
        });

        if !exists {
            let statement = format!("ALTER TABLE {table} ADD COLUMN {column} {definition}");
            sqlx::query(&statement).execute(&self.pool).await?;
        }

        Ok(())
    }
}

fn row_to_job(row: sqlx::sqlite::SqliteRow) -> Result<JobRecord> {
    let id: String = row.get("id");
    let status: String = row.get("status");
    let created_at: String = row.get("created_at");
    let updated_at: String = row.get("updated_at");

    Ok(JobRecord {
        id: Uuid::parse_str(&id)
            .map_err(|error| RkError::BadRequest(format!("invalid stored job id: {error}")))?,
        status: JobStatus::parse(&status)
            .ok_or_else(|| RkError::BadRequest(format!("invalid stored job status `{status}`")))?,
        source_url: row.get("source_url"),
        bitrate: row.get("bitrate"),
        created_at: parse_time(&created_at)?,
        updated_at: parse_time(&updated_at)?,
        status_url: row.get("status_url"),
        media_url: row.get("media_url"),
        error: row.get("error"),
        discovery: DiscoveryMode::parse(&row.get::<String, _>("discovery"))
            .unwrap_or(DiscoveryMode::Direct),
        platform_hint: PlatformHint::parse(&row.get::<String, _>("platform_hint"))
            .unwrap_or(PlatformHint::Auto),
        outputs: parse_outputs_json(&row.get::<String, _>("outputs_json"))?,
        profile_id: row.get("profile_id"),
        auth_mode: AuthMode::parse(&row.get::<String, _>("auth_mode")).unwrap_or(AuthMode::None),
        selected_candidate_ids: parse_uuid_list_json(
            &row.get::<String, _>("selected_candidate_ids_json"),
        )?,
        requester_ip: row.get("requester_ip"),
        requester_user_agent: row.get("requester_user_agent"),
        requester_label: row.get("requester_label"),
        requester_key_id: row
            .get::<Option<String>, _>("requester_key_id")
            .and_then(|raw| Uuid::parse_str(&raw).ok()),
        resolved_extractor: row.get("resolved_extractor"),
        error_class: ErrorClass::parse(&row.get::<String, _>("error_class")),
        attempt_count: row.get("attempt_count"),
        started_at: parse_time_opt(row.get::<Option<String>, _>("started_at"))?,
        completed_at: parse_time_opt(row.get::<Option<String>, _>("completed_at"))?,
    })
}

fn row_to_api_key(row: sqlx::sqlite::SqliteRow) -> Result<ApiKeyRecord> {
    let id: String = row.get("id");
    let role: String = row.get("role");
    let created_at: String = row.get("created_at");

    Ok(ApiKeyRecord {
        id: Uuid::parse_str(&id)
            .map_err(|error| RkError::BadRequest(format!("invalid stored api key id: {error}")))?,
        label: row.get("label"),
        key_hash: row.get("key_hash"),
        key_prefix: row.get("key_prefix"),
        role: ApiKeyRole::parse(&role).unwrap_or(ApiKeyRole::User),
        max_download_bytes: row
            .get::<Option<i64>, _>("max_download_bytes")
            .map(|value| value.max(0) as u64),
        allow_browser_probe: row.get::<i64, _>("allow_browser_probe") != 0,
        allow_ytdlp: row.get::<i64, _>("allow_ytdlp") != 0,
        allow_external_adapters: row.get::<i64, _>("allow_external_adapters") != 0,
        allow_login_profile: row.get::<i64, _>("allow_login_profile") != 0,
        created_at: parse_time(&created_at)?,
        revoked_at: parse_time_opt(row.get::<Option<String>, _>("revoked_at"))?,
    })
}

fn row_to_hidden_batch(row: sqlx::sqlite::SqliteRow) -> Result<HiddenJobBatchView> {
    let id: String = row.get("id");
    let created_at: String = row.get("created_at");

    Ok(HiddenJobBatchView {
        id: Uuid::parse_str(&id)
            .map_err(|error| RkError::BadRequest(format!("invalid hidden batch id: {error}")))?,
        actor_key_id: opt_uuid(row.get("actor_key_id")),
        actor_label: row.get("actor_label"),
        hidden_count: row.get("hidden_count"),
        restored_count: row.get("restored_count"),
        created_at: parse_time(&created_at)?,
        restored_at: parse_time_opt(row.get("restored_at"))?,
    })
}

fn row_to_candidate(row: sqlx::sqlite::SqliteRow) -> Result<MediaCandidate> {
    let id: String = row.get("id");
    let job_id: String = row.get("job_id");
    let kind: String = row.get("kind");
    let metadata_json: String = row.get("metadata_json");
    let created_at: String = row.get("created_at");
    let status: Option<i64> = row.get("status");

    Ok(MediaCandidate {
        id: Uuid::parse_str(&id).map_err(|error| {
            RkError::BadRequest(format!("invalid stored candidate id: {error}"))
        })?,
        job_id: Uuid::parse_str(&job_id)
            .map_err(|error| RkError::BadRequest(format!("invalid stored job id: {error}")))?,
        url: row.get("url"),
        kind: CandidateKind::parse(&kind).unwrap_or(CandidateKind::Unknown),
        extractor: row.get("extractor"),
        method: row.get("method"),
        status: status.and_then(|value| u16::try_from(value).ok()),
        content_type: row.get("content_type"),
        content_length: row.get("content_length"),
        resource_type: row.get("resource_type"),
        initiator_url: row.get("initiator_url"),
        quality_label: row.get("quality_label"),
        score: row.get("score"),
        requires_authorization: row.get::<i64, _>("requires_authorization") != 0,
        platform: row
            .get::<Option<String>, _>("platform")
            .as_deref()
            .and_then(PlatformHint::parse),
        route: row.get("route"),
        extractor_confidence: row.get("extractor_confidence"),
        protection: row
            .get::<Option<String>, _>("protection")
            .as_deref()
            .and_then(CandidateProtection::parse),
        requires_profile: row.get::<i64, _>("requires_profile") != 0,
        ttl_hint_seconds: row.get("ttl_hint_seconds"),
        ad_risk: row.get::<i64, _>("ad_risk") != 0,
        evidence_count: row.get("evidence_count"),
        paired_candidate_ids: parse_uuid_list_json(
            &row.get::<String, _>("paired_candidate_ids_json"),
        )?,
        failure_reason: row.get("failure_reason"),
        validation_state: row
            .get::<Option<String>, _>("validation_state")
            .as_deref()
            .and_then(CandidateValidationState::parse),
        metadata_json: serde_json::from_str(&metadata_json)?,
        created_at: parse_time(&created_at)?,
        score_breakdown_json: serde_json::from_str(&row.get::<String, _>("score_breakdown_json"))
            .unwrap_or_else(|_| serde_json::Value::Object(Default::default())),
        selected: row.get::<i64, _>("selected") != 0,
        selection_reason: row.get("selection_reason"),
        validation_status: row.get("validation_status"),
        resolved_ip: row.get("resolved_ip"),
        final_url_after_redirects: row.get("final_url_after_redirects"),
        expires_at: parse_time_opt(row.get::<Option<String>, _>("expires_at"))?,
        discovered_by_event_id: row
            .get::<Option<String>, _>("discovered_by_event_id")
            .and_then(|raw| Uuid::parse_str(&raw).ok()),
    })
}

fn row_to_artifact(row: sqlx::sqlite::SqliteRow) -> Result<ArtifactView> {
    let id: String = row.get("id");
    let job_id: String = row.get("job_id");
    let kind: String = row.get("kind");
    let created_at: String = row.get("created_at");

    Ok(ArtifactView {
        id: Uuid::parse_str(&id)
            .map_err(|error| RkError::BadRequest(format!("invalid stored artifact id: {error}")))?,
        job_id: Uuid::parse_str(&job_id)
            .map_err(|error| RkError::BadRequest(format!("invalid stored job id: {error}")))?,
        kind: parse_output_kind(&kind)?,
        path: row.get("path"),
        media_url: row.get("media_url"),
        content_type: row.get("content_type"),
        bytes: row.get("bytes"),
        created_at: parse_time(&created_at)?,
    })
}

fn parse_json_or(value: String, fallback: serde_json::Value) -> serde_json::Value {
    serde_json::from_str(&value).unwrap_or(fallback)
}

fn opt_uuid(value: Option<String>) -> Option<Uuid> {
    value.and_then(|raw| Uuid::parse_str(&raw).ok())
}

fn row_to_request(row: sqlx::sqlite::SqliteRow) -> Result<RequestLog> {
    let status: Option<i64> = row.get("response_status");
    Ok(RequestLog {
        id: Uuid::parse_str(&row.get::<String, _>("id"))
            .map_err(|e| RkError::BadRequest(format!("invalid request_log id: {e}")))?,
        job_id: Uuid::parse_str(&row.get::<String, _>("job_id"))
            .map_err(|e| RkError::BadRequest(format!("invalid job id: {e}")))?,
        candidate_id: opt_uuid(row.get("candidate_id")),
        phase: RequestPhase::parse(&row.get::<String, _>("phase")),
        method: row.get("method"),
        url: row.get("url"),
        host: row.get("host"),
        resolved_ip: row.get("resolved_ip"),
        egress_ip: row.get("egress_ip"),
        request_headers_json: parse_json_or(
            row.get("request_headers_json"),
            serde_json::Value::Object(Default::default()),
        ),
        user_agent: row.get("user_agent"),
        referer: row.get("referer"),
        profile_id: row.get("profile_id"),
        response_status: status.and_then(|v| u16::try_from(v).ok()),
        response_headers_json: row
            .get::<Option<String>, _>("response_headers_json")
            .map(|v| parse_json_or(v, serde_json::Value::Null)),
        content_type: row.get("content_type"),
        content_length: row.get("content_length"),
        bytes_read: row.get("bytes_read"),
        redirect_chain_json: row
            .get::<Option<String>, _>("redirect_chain_json")
            .map(|v| parse_json_or(v, serde_json::Value::Null)),
        http_version: row.get("http_version"),
        tls_version: row.get("tls_version"),
        started_at: parse_time(&row.get::<String, _>("started_at"))?,
        ended_at: parse_time_opt(row.get::<Option<String>, _>("ended_at"))?,
        duration_ms: row.get("duration_ms"),
        error_class: ErrorClass::parse(&row.get::<String, _>("error_class")),
        error_message: row.get("error_message"),
    })
}

fn row_to_event(row: sqlx::sqlite::SqliteRow) -> Result<PipelineEvent> {
    Ok(PipelineEvent {
        id: Uuid::parse_str(&row.get::<String, _>("id"))
            .map_err(|e| RkError::BadRequest(format!("invalid event id: {e}")))?,
        job_id: Uuid::parse_str(&row.get::<String, _>("job_id"))
            .map_err(|e| RkError::BadRequest(format!("invalid job id: {e}")))?,
        seq: row.get("seq"),
        stage: row.get("stage"),
        actor: row.get("actor"),
        event_type: PipelineEventType::parse(&row.get::<String, _>("event_type")),
        detail_json: parse_json_or(row.get("detail_json"), serde_json::Value::Null),
        candidate_id: opt_uuid(row.get("candidate_id")),
        request_log_id: opt_uuid(row.get("request_log_id")),
        created_at: parse_time(&row.get::<String, _>("created_at"))?,
        duration_ms: row.get("duration_ms"),
    })
}

fn row_to_browser_session(row: sqlx::sqlite::SqliteRow) -> Result<BrowserSession> {
    Ok(BrowserSession {
        id: Uuid::parse_str(&row.get::<String, _>("id"))
            .map_err(|e| RkError::BadRequest(format!("invalid browser_session id: {e}")))?,
        job_id: Uuid::parse_str(&row.get::<String, _>("job_id"))
            .map_err(|e| RkError::BadRequest(format!("invalid job id: {e}")))?,
        profile_id: row.get("profile_id"),
        user_agent: row.get("user_agent"),
        viewport: row.get("viewport"),
        locale: row.get("locale"),
        timezone: row.get("timezone"),
        headed: row.get::<i64, _>("headed") != 0,
        final_url: row.get("final_url"),
        page_title: row.get("page_title"),
        event_count: row.get("event_count"),
        candidate_count: row.get("candidate_count"),
        playback_triggered: row.get::<i64, _>("playback_triggered") != 0,
        timed_out: row.get::<i64, _>("timed_out") != 0,
        warnings_json: parse_json_or(row.get("warnings_json"), serde_json::json!([])),
        console_errors_json: parse_json_or(row.get("console_errors_json"), serde_json::json!([])),
        screenshot_path: row.get("screenshot_path"),
        started_at: parse_time(&row.get::<String, _>("started_at"))?,
        ended_at: parse_time_opt(row.get::<Option<String>, _>("ended_at"))?,
    })
}

fn row_to_probe(row: sqlx::sqlite::SqliteRow) -> Result<MediaProbe> {
    Ok(MediaProbe {
        id: Uuid::parse_str(&row.get::<String, _>("id"))
            .map_err(|e| RkError::BadRequest(format!("invalid media_probe id: {e}")))?,
        job_id: Uuid::parse_str(&row.get::<String, _>("job_id"))
            .map_err(|e| RkError::BadRequest(format!("invalid job id: {e}")))?,
        candidate_id: opt_uuid(row.get("candidate_id")),
        container: row.get("container"),
        duration_s: row.get("duration_s"),
        overall_bitrate: row.get("overall_bitrate"),
        streams_json: parse_json_or(row.get("streams_json"), serde_json::json!([])),
        raw_json: parse_json_or(row.get("raw_json"), serde_json::Value::Null),
        created_at: parse_time(&row.get::<String, _>("created_at"))?,
    })
}

fn row_to_transcode(row: sqlx::sqlite::SqliteRow) -> Result<TranscodeRun> {
    Ok(TranscodeRun {
        id: Uuid::parse_str(&row.get::<String, _>("id"))
            .map_err(|e| RkError::BadRequest(format!("invalid transcode_run id: {e}")))?,
        job_id: Uuid::parse_str(&row.get::<String, _>("job_id"))
            .map_err(|e| RkError::BadRequest(format!("invalid job id: {e}")))?,
        candidate_id: opt_uuid(row.get("candidate_id")),
        tool: row.get("tool"),
        command_redacted: row.get("command_redacted"),
        input_bytes: row.get("input_bytes"),
        output_bytes: row.get("output_bytes"),
        output_path: row.get("output_path"),
        output_kind: row.get("output_kind"),
        profile: row.get("profile"),
        exit_code: row.get("exit_code"),
        stderr_tail: row.get("stderr_tail"),
        duration_ms: row.get("duration_ms"),
        created_at: parse_time(&row.get::<String, _>("created_at"))?,
    })
}

fn row_to_domain_policy(row: sqlx::sqlite::SqliteRow) -> Result<DomainPolicy> {
    Ok(DomainPolicy {
        host: row.get("host"),
        allow_mode: row.get("allow_mode"),
        max_concurrency: row.get("max_concurrency"),
        crawl_delay_ms: row.get("crawl_delay_ms"),
        requires_user_auth: row.get::<i64, _>("requires_user_auth") != 0,
        last_status: row.get("last_status"),
        blocked_count: row.get("blocked_count"),
        learned_api_pattern: row.get("learned_api_pattern"),
        notes: row.get("notes"),
        updated_at: parse_time(&row.get::<String, _>("updated_at"))?,
    })
}

fn format_time(value: OffsetDateTime) -> Result<String> {
    value
        .format(&Rfc3339)
        .map_err(|error| RkError::Source(format!("failed to format timestamp: {error}")))
}

fn parse_time(value: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|error| RkError::Source(format!("failed to parse stored timestamp: {error}")))
}

fn parse_time_opt(value: Option<String>) -> Result<Option<OffsetDateTime>> {
    value
        .filter(|raw| !raw.is_empty())
        .map(|raw| parse_time(&raw))
        .transpose()
}

fn parse_outputs_json(value: &str) -> Result<Vec<OutputKind>> {
    serde_json::from_str(value).or_else(|_| Ok(vec![OutputKind::Audio]))
}

fn parse_output_kind(value: &str) -> Result<OutputKind> {
    match value {
        "audio" => Ok(OutputKind::Audio),
        "video" => Ok(OutputKind::Video),
        "image" => Ok(OutputKind::Image),
        "markdown" => Ok(OutputKind::Markdown),
        "page_html" => Ok(OutputKind::PageHtml),
        _ => Err(RkError::BadRequest(format!(
            "invalid stored artifact kind `{value}`"
        ))),
    }
}

fn parse_uuid_list_json(value: &str) -> Result<Vec<Uuid>> {
    let values = serde_json::from_str::<Vec<String>>(value).unwrap_or_default();
    values
        .into_iter()
        .map(|value| {
            Uuid::parse_str(&value).map_err(|error| {
                RkError::BadRequest(format!("invalid stored selected candidate id: {error}"))
            })
        })
        .collect()
}

fn hash_api_key(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

fn generate_api_key(prefix: &str) -> String {
    format!(
        "{prefix}_{}{}",
        Uuid::new_v4().simple(),
        Uuid::new_v4().simple()
    )
}

fn normalize_custom_api_key(value: &str) -> Result<String> {
    let key = value.trim();
    if key.len() < 16 {
        return Err(RkError::BadRequest(
            "api key must be at least 16 characters".to_string(),
        ));
    }
    if key.len() > 256 {
        return Err(RkError::BadRequest(
            "api key must be 256 characters or shorter".to_string(),
        ));
    }
    if key
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        return Err(RkError::BadRequest(
            "api key cannot contain whitespace or control characters".to_string(),
        ));
    }
    Ok(key.to_string())
}

fn mib_to_bytes(value: u64) -> u64 {
    value.saturating_mul(1024).saturating_mul(1024)
}

fn normalize_public_base_url(value: &str) -> Result<String> {
    let trimmed = value.trim().trim_end_matches('/');
    let url = url::Url::parse(trimmed)
        .map_err(|error| RkError::BadRequest(format!("invalid public base URL: {error}")))?;
    match url.scheme() {
        "http" | "https" => Ok(trimmed.to_string()),
        _ => Err(RkError::BadRequest(
            "public base URL must use http or https".to_string(),
        )),
    }
}

fn validate_range(name: &str, value: u64, min: u64, max: u64) -> Result<()> {
    if value < min || value > max {
        return Err(RkError::BadRequest(format!(
            "{name} must be between {min} and {max}"
        )));
    }
    Ok(())
}

async fn upsert_runtime_setting(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key: &str,
    value: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO runtime_settings (key, value, updated_at)
        VALUES (?, ?, ?)
        ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(key)
    .bind(value)
    .bind(format_time(OffsetDateTime::now_utc())?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn normalize_api_key_label(value: Option<&str>) -> String {
    let label = value.unwrap_or("用户密钥").trim();
    if label.is_empty() {
        "用户密钥".to_string()
    } else {
        label.chars().take(80).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observability::redact_header_map;
    use std::collections::BTreeMap;

    fn temp_db_path() -> std::path::PathBuf {
        std::env::temp_dir().join(format!("rk-test-{}.db", Uuid::new_v4()))
    }

    #[tokio::test]
    async fn migrates_and_round_trips_full_trace() {
        let path = temp_db_path();
        let store = JobStore::connect(&path).await.unwrap();

        // A job carries requester provenance.
        let record = JobRecord::new(
            "https://www.streetvoice.cn/SpaceStaion/songs/863335/".to_string(),
            "192k".to_string(),
            "http://localhost:8787",
        )
        .with_requester(
            Some("203.0.113.7".to_string()),
            Some("Mozilla/5.0 ReflectionKing".to_string()),
            Some("admin".to_string()),
            None,
        );
        let job_id = record.id;
        store.insert(&record).await.unwrap();

        // One outbound request, with secrets redacted before persistence.
        let mut headers = BTreeMap::new();
        headers.insert("user-agent".to_string(), "Mozilla/5.0".to_string());
        headers.insert("cookie".to_string(), "sv_session=topsecret".to_string());
        let mut request = RequestLog::begin(
            job_id,
            RequestPhase::Probe,
            "GET",
            "https://www.streetvoice.cn/api/v6/songs/863335/",
        );
        request.request_headers_json = redact_header_map(&headers);
        request.resolved_ip = Some("104.18.0.1".to_string());
        request.response_status = Some(200);
        request.complete();
        store.log_request(&request).await.unwrap();

        store
            .log_event(&PipelineEvent::new(
                job_id,
                "resolving",
                "street_voice",
                PipelineEventType::ExtractorAttempt,
                serde_json::json!({ "song_id": "863335" }),
            ))
            .await
            .unwrap();

        store
            .record_probe(&MediaProbe {
                id: Uuid::new_v4(),
                job_id,
                candidate_id: None,
                container: Some("mp3".to_string()),
                duration_s: Some(212.3),
                overall_bitrate: Some(192_000),
                streams_json: serde_json::json!([{ "codec": "mp3" }]),
                raw_json: serde_json::json!({ "format": { "format_name": "mp3" } }),
                created_at: OffsetDateTime::now_utc(),
            })
            .await
            .unwrap();

        store
            .record_transcode(&TranscodeRun {
                id: Uuid::new_v4(),
                job_id,
                candidate_id: None,
                tool: "ffmpeg".to_string(),
                command_redacted: "ffmpeg -i input.media -b:a 192k out.mp3".to_string(),
                input_bytes: Some(1024),
                output_bytes: Some(512),
                output_path: Some("public/out.mp3".to_string()),
                output_kind: Some("audio".to_string()),
                profile: Some("audio_mp3_vrc".to_string()),
                exit_code: Some(0),
                stderr_tail: None,
                duration_ms: Some(1200),
                created_at: OffsetDateTime::now_utc(),
            })
            .await
            .unwrap();

        let mut policy = DomainPolicy::default_for("www.streetvoice.cn");
        policy.learned_api_pattern = Some("/api/v6/songs/{id}/".to_string());
        store.upsert_domain_policy(&policy).await.unwrap();

        store
            .mark_ready(job_id, "http://localhost:8787/media/x/audio.mp3")
            .await
            .unwrap();

        // Job round-trips with provenance + completion timestamp.
        let loaded = store.get(job_id).await.unwrap().unwrap();
        assert_eq!(loaded.requester_ip.as_deref(), Some("203.0.113.7"));
        assert_eq!(loaded.status, JobStatus::Ready);
        assert_eq!(loaded.error_class, ErrorClass::None);
        assert!(loaded.completed_at.is_some());

        // Full trace assembles every observability stream.
        let trace = store.get_trace(job_id).await.unwrap();
        assert_eq!(trace.requests.len(), 1);
        assert_eq!(trace.events.len(), 1);
        assert_eq!(trace.probes.len(), 1);
        assert_eq!(trace.transcodes.len(), 1);
        // Secret never persisted in the request log.
        let req = &trace.requests[0];
        assert_eq!(req.resolved_ip.as_deref(), Some("104.18.0.1"));
        assert!(!req.request_headers_json.to_string().contains("topsecret"));

        let stored_policy = store
            .get_domain_policy("www.streetvoice.cn")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored_policy.learned_api_pattern.as_deref(),
            Some("/api/v6/songs/{id}/")
        );

        drop(store);
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn api_keys_are_hashed_listed_and_revoked() {
        let path = temp_db_path();
        let store = JobStore::connect(&path).await.unwrap();

        let created = store
            .create_user_key(CreateUserKeyRequest {
                label: Some("tester".to_string()),
                key: None,
                max_download_mb: None,
                allow_browser_probe: true,
                allow_ytdlp: false,
                allow_external_adapters: true,
                allow_login_profile: false,
            })
            .await
            .unwrap();

        assert_eq!(created.record.label, "tester");
        assert!(created.record.allow_browser_probe);
        assert!(!created.record.allow_ytdlp);
        assert!(created.key.starts_with("rk_user_"));

        let found = store.find_api_key(&created.key).await.unwrap().unwrap();
        assert_eq!(found.id, created.record.id);
        assert_ne!(found.key_hash, created.key);

        let listed = store.list_api_keys().await.unwrap();
        assert_eq!(listed.len(), 1);

        assert!(store.revoke_api_key(created.record.id).await.unwrap());
        assert!(store.find_api_key(&created.key).await.unwrap().is_none());

        drop(store);
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn api_keys_can_use_custom_secrets_and_limits() {
        let path = temp_db_path();
        let store = JobStore::connect(&path).await.unwrap();

        let custom_user_key = "user-custom-secret-12345";
        let created = store
            .create_user_key(CreateUserKeyRequest {
                label: Some("limited".to_string()),
                key: Some(custom_user_key.to_string()),
                max_download_mb: Some(512),
                allow_browser_probe: false,
                allow_ytdlp: true,
                allow_external_adapters: false,
                allow_login_profile: false,
            })
            .await
            .unwrap();

        assert_eq!(created.key, custom_user_key);
        assert_eq!(created.record.max_download_bytes, Some(512 * 1024 * 1024));
        let found = store.find_api_key(custom_user_key).await.unwrap().unwrap();
        assert_eq!(found.max_download_bytes, Some(512 * 1024 * 1024));
        assert!(found.allow_external_adapters);

        let custom_admin_key = "admin-custom-secret-12345";
        let rotated = store
            .rotate_admin_key(Some(custom_admin_key))
            .await
            .unwrap();
        assert_eq!(rotated.key, custom_admin_key);
        assert_eq!(
            store
                .find_api_key(custom_admin_key)
                .await
                .unwrap()
                .unwrap()
                .role,
            ApiKeyRole::Admin
        );

        drop(store);
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn runtime_settings_are_persisted() {
        let path = temp_db_path();
        let store = JobStore::connect(&path).await.unwrap();

        store
            .update_runtime_settings(UpdateRuntimeSettingsRequest {
                public_base_url: Some("https://rk.example.test/".to_string()),
                max_download_mb: Some(2048),
                download_timeout_seconds: Some(900),
                yt_dlp_timeout_seconds: Some(120),
                yt_dlp_max_json_mb: Some(16),
                job_ttl_hours: Some(72),
                page_archive_max_resources: Some(75),
                page_archive_max_resource_mb: Some(8),
                page_archive_max_total_mb: Some(64),
            })
            .await
            .unwrap();

        let values = store.runtime_setting_values().await.unwrap();
        assert_eq!(
            values.get("public_base_url").map(String::as_str),
            Some("https://rk.example.test")
        );
        assert_eq!(
            values.get("max_download_mb").map(String::as_str),
            Some("2048")
        );
        assert_eq!(
            values.get("download_timeout_seconds").map(String::as_str),
            Some("900")
        );
        assert_eq!(
            values.get("yt_dlp_timeout_seconds").map(String::as_str),
            Some("120")
        );
        assert_eq!(
            values.get("yt_dlp_max_json_mb").map(String::as_str),
            Some("16")
        );
        assert_eq!(values.get("job_ttl_hours").map(String::as_str), Some("72"));
        assert_eq!(
            values.get("page_archive_max_resources").map(String::as_str),
            Some("75")
        );
        assert_eq!(
            values
                .get("page_archive_max_resource_mb")
                .map(String::as_str),
            Some("8")
        );
        assert_eq!(
            values.get("page_archive_max_total_mb").map(String::as_str),
            Some("64")
        );

        drop(store);
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn jobs_can_be_hidden_without_deleting_history() {
        let path = temp_db_path();
        let store = JobStore::connect(&path).await.unwrap();
        let key_id = Uuid::new_v4();

        let first = JobRecord::new(
            "https://example.com/a".to_string(),
            "auto".to_string(),
            "http://localhost:8787",
        )
        .with_requester(None, None, Some("user".to_string()), Some(key_id));
        let second = JobRecord::new(
            "https://example.com/b".to_string(),
            "auto".to_string(),
            "http://localhost:8787",
        );
        let first_id = first.id;
        let second_id = second.id;
        store.insert(&first).await.unwrap();
        store.insert(&second).await.unwrap();

        assert_eq!(store.list_recent(10).await.unwrap().len(), 2);
        assert_eq!(
            store.list_recent_for_key(key_id, 10).await.unwrap().len(),
            1
        );

        let hidden = store
            .hide_visible_jobs_for_key(key_id, Some("user"))
            .await
            .unwrap();
        assert_eq!(hidden.hidden, 1);
        assert!(hidden.batch_id.is_some());
        let user_batches = store
            .list_hidden_job_batches(Some(key_id), 10)
            .await
            .unwrap();
        assert_eq!(user_batches.len(), 1);
        assert_eq!(user_batches[0].hidden_count, 1);
        assert_eq!(store.list_recent(10).await.unwrap().len(), 1);
        assert_eq!(
            store.list_recent_for_key(key_id, 10).await.unwrap().len(),
            0
        );
        assert!(store.get(first_id).await.unwrap().is_some());
        assert!(store.get(second_id).await.unwrap().is_some());

        let restored = store
            .restore_latest_hidden_job_batch(Some(key_id))
            .await
            .unwrap();
        assert_eq!(restored.restored, 1);
        assert_eq!(store.list_recent(10).await.unwrap().len(), 2);

        let hidden = store.hide_visible_jobs(None, Some("admin")).await.unwrap();
        assert_eq!(hidden.hidden, 2);
        assert!(store.list_recent(10).await.unwrap().is_empty());
        assert!(store.get(first_id).await.unwrap().is_some());
        assert!(store.get(second_id).await.unwrap().is_some());

        let restored = store.restore_latest_hidden_job_batch(None).await.unwrap();
        assert_eq!(restored.restored, 2);
        assert_eq!(store.list_recent(10).await.unwrap().len(), 2);

        drop(store);
        std::fs::remove_file(&path).ok();
    }

    #[tokio::test]
    async fn admin_key_can_be_seeded_and_rotated() {
        let path = temp_db_path();
        let store = JobStore::connect(&path).await.unwrap();

        store
            .ensure_admin_key_from_secret("seed-admin-key")
            .await
            .unwrap();
        let seeded = store.find_api_key("seed-admin-key").await.unwrap().unwrap();
        assert_eq!(seeded.role, ApiKeyRole::Admin);

        let rotated = store.rotate_admin_key(None).await.unwrap();
        assert!(rotated.key.starts_with("rk_admin_"));
        assert_eq!(rotated.record.role, ApiKeyRole::Admin);
        assert!(store
            .find_api_key("seed-admin-key")
            .await
            .unwrap()
            .is_none());

        let found = store.find_api_key(&rotated.key).await.unwrap().unwrap();
        assert_eq!(found.id, rotated.record.id);
        assert_eq!(found.role, ApiKeyRole::Admin);

        store
            .ensure_admin_key_from_secret("seed-admin-key")
            .await
            .unwrap();
        assert!(store
            .find_api_key("seed-admin-key")
            .await
            .unwrap()
            .is_none());

        drop(store);
        std::fs::remove_file(&path).ok();
    }
}
