use super::{AsyncPool, PoolError, RepositoryError, Result};
use crate::model::{Session, User};
use crate::schema::user_sessions::dsl as session_dsl;
use crate::schema::users::dsl;
use chrono::Utc;
use diesel::prelude::*;
use diesel::result::Error;
use std::sync::Arc;
use std::time::Duration;

pub mod models {
    use crate::schema::user_sessions;
    use crate::schema::users;

    #[derive(Queryable)]
    pub struct User {
        pub id: i32,
        pub username: String,
        pub password: String,
    }

    #[derive(Insertable)]
    #[diesel(table_name = users)]
    pub struct NewUser {
        pub username: String,
        pub password: String,
    }

    #[derive(Identifiable)]
    #[diesel(table_name = user_sessions)]
    pub struct UserID {
        pub id: i32,
    }

    #[derive(Queryable, Insertable, Identifiable, Associations, AsChangeset)]
    #[diesel(table_name = user_sessions, belongs_to(UserID, foreign_key = user_id))]
    pub struct Session {
        pub id: String,
        pub user_id: i32,
        pub expiration_ms: i64,
        pub last_use_ms: i64,
    }
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait UserRepository {
    /// It takes a username and a password to create a new user. The username
    /// must be unique.
    async fn create(&self, user: models::NewUser) -> Result<()>;

    /// It returns the user associated with the username if it exists.
    async fn get(&self, username: &str) -> Result<User>;

    async fn create_session(&self, user: &User, exp: Duration) -> Result<Session>;

    async fn use_session(&self, user: &User, current_time_ms: i64) -> Result<Session>;
}

pub type DynUserRepository = Arc<dyn UserRepository + Sync + Send>;

/// A repository to support the user operations, backed by SQLite.
pub struct SqliteUserRepository {
    pool: AsyncPool,
}

impl SqliteUserRepository {
    pub fn new(pool: AsyncPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl UserRepository for SqliteUserRepository {
    async fn create(&self, user: models::NewUser) -> Result<()> {
        self.pool
            .exec(move |mut conn| {
                diesel::insert_into(dsl::users)
                    .values(&user)
                    .execute(&mut conn)
            })
            .await?;

        Ok(())
    }

    async fn get(&self, username: &str) -> Result<User> {
        let username = username.to_string();

        let ret: models::User = self
            .pool
            .exec(|mut conn| {
                dsl::users
                    .filter(dsl::username.eq(username))
                    .first(&mut conn)
            })
            .await
            .map_err(|e| match e {
                PoolError::Driver(Error::NotFound) => RepositoryError::NotFound,
                _ => RepositoryError::from(e),
            })?;

        Ok(User::from(ret))
    }

    /// It creates a session for a user using the given parameters. If a session
    /// already exists, it will be overwritten.
    async fn create_session(&self, user: &User, exp: Duration) -> Result<Session> {
        let now = Utc::now();
        let exp = chrono::Duration::from_std(exp).unwrap_or(chrono::Duration::zero());

        let new_session = models::Session {
            id: uuid::Uuid::new_v4().hyphenated().to_string(),
            user_id: user.get_id(),
            expiration_ms: (now + exp).timestamp_millis(),
            last_use_ms: now.timestamp_millis(),
        };

        let res = self
            .pool
            .exec(move |mut conn| {
                conn.transaction(|tx| {
                    diesel::replace_into(session_dsl::user_sessions)
                        .values(&new_session)
                        .on_conflict(session_dsl::user_id)
                        .do_update()
                        .set(&new_session)
                        .execute(tx)?;

                    let user = models::UserID {
                        id: new_session.user_id,
                    };

                    models::Session::belonging_to(&user).first::<models::Session>(tx)
                })
            })
            .await?;

        Ok(Session::from(res))
    }

    /// It looks for a session for the user and update the last use field if
    /// found then returns it. If the session is expired, an error is returned.
    async fn use_session(&self, user: &User, current_time_ms: i64) -> Result<Session> {
        let user = models::UserID { id: user.get_id() };

        let ret: models::Session = self
            .pool
            .exec(move |mut conn| {
                conn.transaction(|tx| {
                    let mut session: models::Session = models::Session::belonging_to(&user)
                        .filter(session_dsl::user_id.eq(user.id))
                        .filter(session_dsl::expiration_ms.gt(current_time_ms))
                        .first(tx)?;

                    session.last_use_ms = current_time_ms;

                    diesel::update(session_dsl::user_sessions)
                        .set(&session)
                        .execute(tx)?;

                    Ok(session)
                })
            })
            .await
            .map_err(|e| match e {
                PoolError::Driver(Error::NotFound) => RepositoryError::NotFound,
                _ => RepositoryError::from(e),
            })?;

        Ok(Session::from(ret))
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repository::{AsyncPool, RepositoryError};
    use std::time::Duration;
    use chrono::Utc;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_user() {
        let pool = AsyncPool::open(":memory:").unwrap();

        pool.run_migrations().await.unwrap();

        let repository = SqliteUserRepository::new(pool);

        let new_user = models::NewUser {
            username: "bob".into(),
            password: "some-hash".into(),
        };

        assert!(matches!(repository.create(new_user).await, Ok(_)));

        let res = repository.get("bob").await;

        assert!(matches!(res, Ok(user) if user.get_username() == "bob"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_user_not_found() {
        let pool = AsyncPool::open(":memory:").unwrap();

        pool.run_migrations().await.unwrap();

        let repository = SqliteUserRepository::new(pool);

        let res = repository.get("bob").await;

        assert!(matches!(res, Err(RepositoryError::NotFound)));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_create_and_use_session() {
        let pool = AsyncPool::open(":memory:").unwrap();

        pool.run_migrations().await.unwrap();

        let repository = SqliteUserRepository::new(pool);

        let user = {
            let new_user = models::NewUser {
                username: "bob".into(),
                password: "some-hash".into(),
            };

            repository.create(new_user).await.unwrap();

            repository.get("bob").await.unwrap()
        };

        let session = repository
            .create_session(&user, Duration::from_secs(30))
            .await
            .unwrap();

        let res = repository.use_session(&user, 700).await;

        assert!(
            matches!(res, Ok(s) if session.get_refresh_token() == s.get_refresh_token())
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_use_expired_session() {
        let pool = AsyncPool::open(":memory:").unwrap();

        pool.run_migrations().await.unwrap();

        let repository = SqliteUserRepository::new(pool);

        let user = {
            let new_user = models::NewUser {
                username: "bob".into(),
                password: "some-hash".into(),
            };

            repository.create(new_user).await.unwrap();

            repository.get("bob").await.unwrap()
        };

        let res = repository
            .create_session(&user, Duration::from_secs(30))
            .await;

        assert!(matches!(res, Ok(_)));

        let res = repository.use_session(&user, Utc::now().timestamp_millis() + 60000).await;

        assert!(matches!(res, Err(_)));
    }
}
