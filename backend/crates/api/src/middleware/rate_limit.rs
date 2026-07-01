use std::net::IpAddr;
use std::num::NonZeroU32;
use std::sync::Arc;

use axum::{
    extract::{ConnectInfo, Extension, Request},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use governor::{DefaultKeyedRateLimiter, Quota, RateLimiter};
use shipyard_common::types::ApiResponse;

/// Shared per-IP rate limiter backed by DashMap.
pub type SharedRateLimiter = Arc<DefaultKeyedRateLimiter<IpAddr>>;

/// Create a new rate limiter: 300 requests per minute per IP.
pub fn make_rate_limiter() -> SharedRateLimiter {
    let quota = Quota::per_minute(NonZeroU32::new(300).expect("quota > 0"));
    Arc::new(RateLimiter::keyed(quota))
}

/// Tighter rate limiter for auth endpoints: 10 requests per minute per IP.
/// Apply to /auth/login and /auth/register to slow credential brute-force.
pub fn make_auth_rate_limiter() -> SharedRateLimiter {
    let quota = Quota::per_minute(NonZeroU32::new(10).expect("quota > 0"));
    Arc::new(RateLimiter::keyed(quota))
}

/// Axum middleware: rejects requests from IPs that exceed the rate limit.
pub async fn rate_limit(
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    Extension(limiter): Extension<SharedRateLimiter>,
    req: Request,
    next: Next,
) -> Response {
    let ip = addr.ip();

    if limiter.check_key(&ip).is_err() {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            Json(ApiResponse::<()>::err(
                "RATE_LIMITED",
                "Too many requests. Please slow down.",
            )),
        )
            .into_response();
    }

    next.run(req).await
}
