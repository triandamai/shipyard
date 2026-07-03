use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// RFC 7807 Problem Details response body.
#[derive(Debug, Serialize)]
pub struct ProblemDetail {
    #[serde(rename = "type")]
    pub type_uri: String,
    pub title: String,
    pub status: u16,
    pub detail: String,
}

/// Unified error type for the Open API crate.
#[derive(Debug, thiserror::Error)]
pub enum OpenApiError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Unauthorized(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Database error: {0}")]
    Database(String),
}

impl OpenApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_)    => StatusCode::NOT_FOUND,
            Self::Unauthorized(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_)   => StatusCode::FORBIDDEN,
            Self::BadRequest(_)  => StatusCode::BAD_REQUEST,
            Self::Conflict(_)    => StatusCode::CONFLICT,
            Self::Internal(_)
            | Self::Database(_)  => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn title(&self) -> &'static str {
        match self {
            Self::NotFound(_)    => "Not Found",
            Self::Unauthorized(_) => "Unauthorized",
            Self::Forbidden(_)   => "Forbidden",
            Self::BadRequest(_)  => "Bad Request",
            Self::Conflict(_)    => "Conflict",
            Self::Internal(_)    => "Internal Server Error",
            Self::Database(_)    => "Internal Server Error",
        }
    }
}

impl IntoResponse for OpenApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ProblemDetail {
            type_uri: format!(
                "https://shipyard.io/errors/{}",
                self.title().to_lowercase().replace(' ', "-")
            ),
            title: self.title().to_string(),
            status: status.as_u16(),
            detail: self.to_string(),
        };
        (status, Json(body)).into_response()
    }
}

impl From<sqlx::Error> for OpenApiError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => Self::NotFound("Resource not found".into()),
            other => Self::Database(other.to_string()),
        }
    }
}
