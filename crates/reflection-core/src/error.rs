use thiserror::Error;

pub type Result<T> = std::result::Result<T, RkError>;

#[derive(Debug, Error)]
pub enum RkError {
    #[error("invalid request: {0}")]
    BadRequest(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("not found: {0}")]
    NotFound(String),

    #[error("remote source error: {0}")]
    Source(String),

    #[error("browser probe error: {0}")]
    Browser(String),

    #[error("URL policy denied request: {0}")]
    UrlPolicy(String),

    #[error("download exceeded configured limit of {max_bytes} bytes")]
    DownloadTooLarge { max_bytes: u64 },

    #[error("requested byte range cannot be satisfied")]
    RangeNotSatisfiable { file_len: u64 },

    #[error("transcode failed: {0}")]
    Transcode(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("URL parse error: {0}")]
    UrlParse(#[from] url::ParseError),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
