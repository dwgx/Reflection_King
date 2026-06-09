use std::path::Path;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use uuid::Uuid;

use crate::{
    models::{
        ArtifactView, AuthMode, CandidateKind, DiscoveryMode, JobRecord, JobStatus, MediaCandidate,
        OutputKind, PlatformHint,
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
                selected_candidate_ids_json
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
                   selected_candidate_ids_json
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
                   selected_candidate_ids_json
            FROM jobs
            ORDER BY created_at DESC
            LIMIT ?
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(row_to_job).collect()
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
                    metadata_json,
                    created_at
                )
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
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
            .bind(candidate.metadata_json.to_string())
            .bind(format_time(candidate.created_at)?)
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
                   metadata_json,
                   created_at
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
                   metadata_json,
                   created_at
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
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = ?,
                media_url = ?,
                error = NULL,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(JobStatus::Ready.as_str())
        .bind(media_url)
        .bind(format_time(OffsetDateTime::now_utc())?)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn mark_error(&self, id: Uuid, error: &str) -> Result<()> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = ?,
                error = ?,
                updated_at = ?
            WHERE id = ?
            "#,
        )
        .bind(JobStatus::Error.as_str())
        .bind(error)
        .bind(format_time(OffsetDateTime::now_utc())?)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    pub async fn recover_pending(&self) -> Result<Vec<Uuid>> {
        sqlx::query(
            r#"
            UPDATE jobs
            SET status = ?,
                updated_at = ?
            WHERE status IN (?, ?, ?, ?, ?, ?, ?)
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
            "CREATE INDEX IF NOT EXISTS idx_jobs_status_created_at ON jobs(status, created_at)",
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
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL
            )
            "#,
        )
        .execute(&self.pool)
        .await?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS idx_source_candidates_job_score ON source_candidates(job_id, score)",
        )
        .execute(&self.pool)
        .await?;

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
        metadata_json: serde_json::from_str(&metadata_json)?,
        created_at: parse_time(&created_at)?,
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

fn format_time(value: OffsetDateTime) -> Result<String> {
    value
        .format(&Rfc3339)
        .map_err(|error| RkError::Source(format!("failed to format timestamp: {error}")))
}

fn parse_time(value: &str) -> Result<OffsetDateTime> {
    OffsetDateTime::parse(value, &Rfc3339)
        .map_err(|error| RkError::Source(format!("failed to parse stored timestamp: {error}")))
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
