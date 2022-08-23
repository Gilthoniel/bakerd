use crate::controller::AppError;
use axum::{
    http::{self, Request},
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
) -> Result<Response, AppError> {
    let header = req
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    match header {
        Some(header) if token_is_valid(header, &secret) => Ok(next.run(req).await),
        _ => Err(AppError::Unauthorized),
    }
}

fn token_is_valid(token: &str, secret: &str) -> bool {
    token.strip_prefix("Bearer ") == Some(secret)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::StatusCode, middleware, routing::get, Router};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_authentication_ok() {
        let secret = Arc::new(String::from("secret"));

        let app = Router::new()
            .route("/", get(|| async { () }))
            .layer(middleware::from_fn(move |req, next| {
                authentication(req, next, secret.clone())
            }));

        let res = app
            .oneshot(
                Request::builder()
                    .header(http::header::AUTHORIZATION, "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;

        assert!(matches!(res, Ok(response) if response.status() == StatusCode::OK));
    }

    #[tokio::test]
    async fn test_authentication_unauthorized() {
        let secret = Arc::new(String::from("secret"));

        let app = Router::new()
            .route("/", get(|| async { () }))
            .layer(middleware::from_fn(move |req, next| {
                authentication(req, next, secret.clone())
            }));

        let res = app
            .oneshot(Request::builder().body(Body::empty()).unwrap())
            .await;

        assert!(matches!(res, Ok(response) if response.status() == StatusCode::UNAUTHORIZED));
    }
}
