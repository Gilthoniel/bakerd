use axum::{
    http::{self, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

pub enum AuthError {
    Malformed,
    Invalid,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let message = match self {
            Self::Malformed => "token is malformed",
            Self::Invalid => "token is invalid",
        };

        let body = Json(json!({
            "code": StatusCode::UNAUTHORIZED.as_u16(),
            "error": message,
        }));

        (StatusCode::UNAUTHORIZED, body).into_response()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {}

/// A middleware that will extract the authentication token and return an error
/// if it does not match.
pub async fn authentication<B>(
    req: Request<B>,
    next: Next<B>,
    key: Arc<DecodingKey>,
) -> Result<Response, AuthError> {
    let header = req
        .headers()
        .get(http::header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    match header {
        Some(header) => {
            token_is_valid(header, key)?;

            Ok(next.run(req).await)
        }
        None => Err(AuthError::Malformed),
    }
}

fn token_is_valid(token: &str, key: Arc<DecodingKey>) -> Result<(), AuthError> {
    match token.strip_prefix("Bearer ") {
        Some(token) => {
            decode::<Claims>(token, &key, &Validation::new(Algorithm::HS256))
                .map_err(|_| AuthError::Invalid)?;

            Ok(())
        }
        None => Err(AuthError::Malformed),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::StatusCode, middleware, routing::get, Router};
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_authentication_ok() {
        let secret = Arc::new(DecodingKey::from_secret(b"some-secret-for-hmac"));

        let app = Router::new()
            .route("/", get(|| async { () }))
            .layer(middleware::from_fn(move |req, next| {
                authentication(req, next, secret.clone())
            }));

        let res = app
            .oneshot(
                Request::builder()
                    .header(http::header::AUTHORIZATION, "Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjMyNTIzOTk5OTgxfQ.4JAGbE50aCK_GI93JmBuY9yQXCGs6VBaWW5QCcOsgDM")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;

        assert!(matches!(res, Ok(response) if response.status() == StatusCode::OK));
    }

    #[tokio::test]
    async fn test_authentication_unauthorized() {
        let secret = Arc::new(DecodingKey::from_secret(b"some-secret-for-hmac"));

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
