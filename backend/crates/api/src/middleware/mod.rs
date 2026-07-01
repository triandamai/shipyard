pub mod audit;
pub mod rate_limit;
pub mod rbac;

use axum::{extract::Request, middleware::Next, response::Response};

#[allow(dead_code)]
pub async fn trace_request(req: Request, next: Next) -> Response {
    let method = req.method().clone();
    let path = req.uri().path().to_string();
    tracing::info!("{} {}", method, path);
    next.run(req).await
}
