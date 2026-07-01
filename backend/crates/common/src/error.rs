use std::fmt;

/// Unified error type for the Shipyard platform.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Docker error: {0}")]
    Docker(String),

    #[error("MQTT error: {0}")]
    Mqtt(String),

    #[error("Git error: {0}")]
    Git(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Platform not initialized")]
    NotInitialized,

    #[error("Rate limit exceeded: {0}")]
    RateLimit(String),
}

/// Convenience type alias.
pub type AppResult<T> = Result<T, AppError>;

/// Structured API error response body.
#[derive(Debug, serde::Serialize)]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
}

impl AppError {
    /// Returns the HTTP status code for this error.
    pub fn status_code(&self) -> u16 {
        match self {
            AppError::NotFound(_) => 404,
            AppError::Unauthorized(_) => 401,
            AppError::Forbidden(_) => 403,
            AppError::BadRequest(_) | AppError::Validation(_) => 400,
            AppError::Conflict(_) => 409,
            AppError::NotInitialized => 503,
            AppError::RateLimit(_) => 429,
            _ => 500,
        }
    }

    /// Returns the error code string for API responses.
    pub fn error_code(&self) -> &'static str {
        match self {
            AppError::NotFound(_) => "NOT_FOUND",
            AppError::Unauthorized(_) => "UNAUTHORIZED",
            AppError::Forbidden(_) => "FORBIDDEN",
            AppError::BadRequest(_) => "BAD_REQUEST",
            AppError::Conflict(_) => "CONFLICT",
            AppError::Internal(_) => "INTERNAL_ERROR",
            AppError::Database(_) => "DATABASE_ERROR",
            AppError::Docker(_) => "DOCKER_ERROR",
            AppError::Mqtt(_) => "MQTT_ERROR",
            AppError::Git(_) => "GIT_ERROR",
            AppError::Config(_) => "CONFIG_ERROR",
            AppError::Validation(_) => "VALIDATION_ERROR",
            AppError::NotInitialized => "NOT_INITIALIZED",
            AppError::RateLimit(_) => "RATE_LIMITED",
        }
    }

    /// Convert to an API error response.
    pub fn to_response(&self) -> ErrorResponse {
        ErrorResponse {
            code: self.error_code().to_string(),
            message: self.to_string(),
        }
    }
}

impl fmt::Display for ErrorResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}
