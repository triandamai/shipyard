use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

/// Newtype wrapper so we can implement `IntoResponse` for `AppError`
/// without violating the orphan rules (AppError is from shipyard_common).
pub struct ApiAppError(pub AppError);

impl From<AppError> for ApiAppError {
    fn from(e: AppError) -> Self {
        ApiAppError(e)
    }
}

impl IntoResponse for ApiAppError {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.0.status_code())
            .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        let body = Json(ApiResponse::<()>::err(self.0.error_code(), self.0.to_string()));
        (status, body).into_response()
    }
}
