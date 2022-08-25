use super::{AsyncPool, PoolError, RepositoryError, Result};
use crate::model::User;
use crate::schema::users::dsl;
use diesel::prelude::*;
use diesel::result::Error;
use std::sync::Arc;

pub mod models {
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
}

#[async_trait]
pub trait UserRepository {
    async fn create(&self, user: models::NewUser) -> Result<()>;

    async fn get(&self, username: &str) -> Result<User>;
}

pub type DynUserRepository = Arc<dyn UserRepository + Sync + Send>;

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
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repository::{AsyncPool, RepositoryError};

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
}
