use axum::extract::{FromRequest, RequestParts};
use axum::{
    headers::{authorization::Bearer, Authorization},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json, TypedHeader,
};
use jsonwebtoken::*;
use log::warn;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

const DEFAULT_EXPIRATION: u64 = 1 * 60 * 60;

type BearerHeader = TypedHeader<Authorization<Bearer>>;

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
pub struct Claims {
    user_id: i32,
    exp: u64,
}

impl Claims {
    pub fn new(user_id: i32) -> Self {
        Self {
            user_id,
            exp: jsonwebtoken::get_current_timestamp() + DEFAULT_EXPIRATION,
        }
    }

    pub fn user_id(&self) -> i32 {
        self.user_id
    }

    /// It returns the timestamp in milliseconds of the expiration of the token.
    pub fn expiration(&self) -> i64 {
        self.exp.try_into().unwrap_or(0) * 1000
    }
}

#[async_trait]
impl<S: Send + Sync> FromRequest<S> for Claims {
    type Rejection = AuthError;

    async fn from_request(req: &mut RequestParts<S>) -> Result<Self, Self::Rejection> {
        let bearer = BearerHeader::from_request(req).await.map_err(|e| {
            warn!("malformed token: {}", e);
            AuthError::Malformed
        })?;

        let key = req.extensions().get::<Arc<DecodingKey>>().unwrap();

        let val = Validation::default();

        let token_data = decode::<Claims>(bearer.token(), key, &val).map_err(|e| {
            warn!("invalid token: {}", e);
            AuthError::Invalid
        })?;

        Ok(token_data.claims)
    }
}

/// It takes an encoding key and create a valid JSON Web Token.
pub fn generate_token(claims: &Claims, key: &EncodingKey) -> errors::Result<String> {
    encode(&Header::default(), &claims, key)
}

/// It takes a password and returns a BCrypt string of the hash.
pub fn hash_password(password: &str) -> String {
    bcrypt::hash(password, 8).unwrap_or(String::default())
}

/// It takes a password and the BCrypt hash and returns true when they both
/// match.
pub fn verify_password(password: &str, hash: &str) -> bool {
    bcrypt::verify(password, hash).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::Extension,
        http::{self, Request, StatusCode},
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_authentication_ok() {
        let decoding_key = Arc::new(DecodingKey::from_secret(b"some-secret-for-hmac"));
        let encoding_key = Arc::new(EncodingKey::from_secret(b"some-secret-for-hmac"));

        let app = Router::new()
            .route("/", get(|_: Claims| async { () }))
            .layer(Extension(decoding_key));

        let token = generate_token(&Claims::new(0), &encoding_key).unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;

        assert!(matches!(res, Ok(response) if response.status() == StatusCode::OK));
    }

    #[tokio::test]
    async fn test_authentication_invalid() {
        let decoding_key = Arc::new(DecodingKey::from_secret(b"some-secret-for-hmac"));
        let encoding_key = Arc::new(EncodingKey::from_secret(b"another-secret-for-hmac"));

        let app = Router::new()
            .route("/", get(|_: Claims| async { () }))
            .layer(Extension(decoding_key));

        let token = generate_token(&Claims::new(0), &encoding_key).unwrap();

        let res = app
            .oneshot(
                Request::builder()
                    .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;

        assert!(matches!(res, Ok(response) if response.status() == StatusCode::UNAUTHORIZED));
    }

    #[tokio::test]
    async fn test_authentication_malformed() {
        let decoding_key = Arc::new(DecodingKey::from_secret(b"some-secret-for-hmac"));

        let app = Router::new()
            .route("/", get(|_: Claims| async { () }))
            .layer(Extension(decoding_key));

        let res = app
            .oneshot(
                Request::builder()
                    .header(http::header::AUTHORIZATION, format!("Bearer oops"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await;

        assert!(matches!(res, Ok(response) if response.status() == StatusCode::UNAUTHORIZED));
    }

    #[tokio::test]
    async fn test_authentication_no_header() {
        let secret = Arc::new(DecodingKey::from_secret(b"some-secret-for-hmac"));

        let app = Router::new()
            .route("/", get(|_: Claims| async { () }))
            .layer(Extension(secret));

        let res = app
            .oneshot(Request::builder().body(Body::empty()).unwrap())
            .await;

        assert!(matches!(res, Ok(response) if response.status() == StatusCode::UNAUTHORIZED));
    }

    #[test]
    fn test_hash_and_verify_password() {
        let hash = hash_password("password");

        assert!(verify_password("password", &hash));
    }
}
