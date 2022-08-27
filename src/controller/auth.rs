use super::AppError;
use crate::authentication::{generate_token, hash_password, Claims};
use crate::model::{Session, User};
use crate::repository::{models, DynUserRepository, RepositoryError};
use axum::{extract::Extension, Json};
use chrono::Utc;
use jsonwebtoken::EncodingKey;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Deserialize, Debug)]
pub struct Credentials {
    username: String,
    password: String,
}

#[derive(Serialize, Debug)]
pub struct Authentication {
    access_token: String,
    refresh_token: String,
    expires_at: i64,
}

/// A controller to authorize users. It takes a username and a password and
/// tries to match them against the users in the database.
pub async fn authorize(
    request: Json<Credentials>,
    Extension(repository): Extension<DynUserRepository>,
    Extension(key): Extension<Arc<EncodingKey>>,
) -> Result<Json<Authentication>, AppError> {
    // 1. Make sure the user exists.
    let user = repository
        .get(&request.username)
        .await
        .map_err(map_wrong_creds)?;

    // 2. Check that the password matches the hash in the storage.
    if !user.check_password(&request.password) {
        return Err(AppError::WrongCredentials);
    }

    // 3. Create a new session that will allow the user to refresh the access
    //    token after it expires.
    let session = repository
        .create_session(&user, Utc::now().timestamp_millis())
        .await
        .map_err(|e| {
            error!("unable to create session: {}", e);
            AppError::Internal
        })?;

    make_token(&user, &session, &key)
}

#[derive(Deserialize, Debug)]
pub struct Token {
    refresh_token: String,
}

pub async fn refresh_token(
    Extension(repository): Extension<DynUserRepository>,
    Extension(key): Extension<Arc<EncodingKey>>,
    claims: Claims,
    request: Json<Token>,
) -> Result<Json<Authentication>, AppError> {
    let user = repository
        .get_by_id(claims.user_id())
        .await
        .map_err(map_wrong_creds)?;

    let now = Utc::now().timestamp_millis();

    let session = repository
        .use_session(&request.refresh_token, &user, now)
        .await
        .map_err(|e| {
            error!("unable to use the session [{}]: {}", request.refresh_token, e);
            AppError::WrongCredentials
        })?;

    make_token(&user, &session, &key)
}

/// A controller to create a user. It takes a username and a password and stores
/// the new user in the database after hashing the password with BCrypt.
pub async fn create_user(
    request: Json<Credentials>,
    Extension(repository): Extension<DynUserRepository>,
    _: Claims,
) -> Result<Json<User>, AppError> {
    let new_user = models::NewUser {
        username: request.username.clone(),
        password: hash_password(&request.password),
    };

    repository.create(new_user).await.map_err(|e| {
        error!("unable to create a user: {}", e);
        AppError::Internal
    })?;

    let user = repository.get(&request.username).await.map_err(|e| {
        error!("unable to get new user: {}", e);
        AppError::Internal
    })?;

    Ok(user.into())
}

fn make_token(
    user: &User,
    session: &Session,
    key: &EncodingKey,
) -> Result<Json<Authentication>, AppError> {
    let claims = Claims::new(user.get_id());

    match generate_token(&claims, &key) {
        Ok(token) => {
            debug!("user [{}] has been authenticated", user.get_username());

            let ret = Authentication {
                access_token: token,
                refresh_token: session.get_refresh_token().into(),
                expires_at: claims.expiration(),
            };

            Ok(ret.into())
        }
        Err(e) => {
            error!("unable to generate a token: {}", e);

            Err(AppError::Internal)
        }
    }
}

fn map_wrong_creds(e: RepositoryError) -> AppError {
    match e {
        RepositoryError::NotFound => AppError::WrongCredentials,
        RepositoryError::Faillable(e) => {
            error!("unable to find user: {}", e);

            AppError::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Session;
    use crate::repository::{models, MockUserRepository};
    use jsonwebtoken::EncodingKey;
    use mockall::predicate::*;
    use std::fmt;

    #[derive(Debug)]
    struct FakeError;

    impl fmt::Display for FakeError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "fake")
        }
    }

    impl std::error::Error for FakeError {}

    #[tokio::test]
    async fn test_authorize() {
        let encoding_key = Arc::new(EncodingKey::from_secret(b"secret"));

        let mut repository = MockUserRepository::new();

        repository.expect_get().times(1).returning(|_| {
            Ok(User::from(models::User {
                id: 1,
                username: "bob".to_string(),
                password: hash_password("password"),
            }))
        });

        repository
            .expect_create_session()
            .times(1)
            .returning(|_, _| {
                Ok(Session::from(models::Session {
                    id: "refresh-token".into(),
                    user_id: 1,
                    expiration_ms: 0,
                    last_use_ms: 0,
                }))
            });

        let creds = Credentials {
            username: "bob".to_string(),
            password: "password".to_string(),
        };

        let res = authorize(
            creds.into(),
            Extension(Arc::new(repository)),
            Extension(encoding_key),
        );

        assert!(matches!(res.await, Ok(_)));
    }

    #[tokio::test]
    async fn test_authorize_no_user() {
        let encoding_key = Arc::new(EncodingKey::from_secret(b"secret"));

        let mut repository = MockUserRepository::new();

        repository
            .expect_get()
            .times(1)
            .returning(|_| Err(RepositoryError::NotFound));

        let creds = Credentials {
            username: "bob".to_string(),
            password: "password".to_string(),
        };

        let res = authorize(
            creds.into(),
            Extension(Arc::new(repository)),
            Extension(encoding_key),
        );

        assert!(matches!(res.await, Err(AppError::WrongCredentials)));
    }

    #[tokio::test]
    async fn test_authorize_storage_failure() {
        let encoding_key = Arc::new(EncodingKey::from_secret(b"secret"));

        let mut repository = MockUserRepository::new();

        repository
            .expect_get()
            .times(1)
            .returning(|_| Err(RepositoryError::Faillable(Box::new(FakeError {}))));

        let creds = Credentials {
            username: "bob".to_string(),
            password: "password".to_string(),
        };

        let res = authorize(
            creds.into(),
            Extension(Arc::new(repository)),
            Extension(encoding_key),
        );

        assert!(matches!(res.await, Err(AppError::Internal)));
    }

    #[tokio::test]
    async fn test_authorize_wrong_password() {
        let encoding_key = Arc::new(EncodingKey::from_secret(b"secret"));

        let mut repository = MockUserRepository::new();

        repository.expect_get().times(1).returning(|_| {
            Ok(User::from(models::User {
                id: 1,
                username: "bob".to_string(),
                password: hash_password("password"),
            }))
        });

        let creds = Credentials {
            username: "bob".to_string(),
            password: "oops".to_string(),
        };

        let res = authorize(
            creds.into(),
            Extension(Arc::new(repository)),
            Extension(encoding_key),
        );

        assert!(matches!(res.await, Err(AppError::WrongCredentials)));
    }

    #[tokio::test]
    async fn test_create_user() {
        let mut repository = MockUserRepository::new();

        repository.expect_create().times(1).returning(|_| Ok(()));

        repository
            .expect_get()
            .with(eq("bob"))
            .times(1)
            .returning(|_| {
                Ok(User::from(models::User {
                    id: 1,
                    username: "bob".to_string(),
                    password: hash_password("password"),
                }))
            });

        let creds = Credentials {
            username: "bob".to_string(),
            password: "password".to_string(),
        };

        let res = create_user(
            creds.into(),
            Extension(Arc::new(repository)),
            Claims::new(0),
        );

        assert!(matches!(res.await, Ok(_)));
    }

    #[tokio::test]
    async fn test_create_user_fail_to_create() {
        let mut repository = MockUserRepository::new();

        repository
            .expect_create()
            .times(1)
            .returning(|_| Err(RepositoryError::NotFound));

        let creds = Credentials {
            username: "bob".to_string(),
            password: "password".to_string(),
        };

        let res = create_user(
            creds.into(),
            Extension(Arc::new(repository)),
            Claims::new(0),
        );

        assert!(matches!(res.await, Err(AppError::Internal)));
    }

    #[tokio::test]
    async fn test_create_user_fail_to_get() {
        let mut repository = MockUserRepository::new();

        repository.expect_create().times(1).returning(|_| Ok(()));

        repository
            .expect_get()
            .with(eq("bob"))
            .times(1)
            .returning(|_| Err(RepositoryError::NotFound));

        let creds = Credentials {
            username: "bob".to_string(),
            password: "password".to_string(),
        };

        let res = create_user(
            creds.into(),
            Extension(Arc::new(repository)),
            Claims::new(0),
        );

        assert!(matches!(res.await, Err(AppError::Internal)));
    }
}
