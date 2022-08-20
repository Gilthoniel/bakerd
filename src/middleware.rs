use axum::{
    http::{self, Request, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// A middleware that will extract the authentication token and return an error
/// if it does not match.
pub async fn authentication<B>(
    req: Request<B>,
    next: Next<B>,
    secret: Arc<String>,
) -> Result<Response, StatusCode> {
    let header = req
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    match header {
        Some(header) if token_is_valid(header, &secret) => Ok(next.run(req).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

fn token_is_valid(token: &str, secret: &str) -> bool {
    token.strip_prefix("Bearer ") == Some(secret)
}
