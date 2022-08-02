pub mod account;

use async_trait::async_trait;
use diesel::r2d2::ConnectionManager;
use diesel::result::Error as DriverError;
use diesel::{QueryResult, SqliteConnection};
use diesel_migrations::RunMigrationsError;
use std::path::Path;

use crate::model::Account;

diesel_migrations::embed_migrations!();

type Connection = r2d2::PooledConnection<ConnectionManager<SqliteConnection>>;

#[derive(Debug)]
pub enum RepoError {
    Pool(r2d2::Error),
    Driver(DriverError),
    Migration(RunMigrationsError),
}

impl From<r2d2::Error> for RepoError {
    fn from(e: r2d2::Error) -> Self {
        Self::Pool(e)
    }
}

impl From<DriverError> for RepoError {
    fn from(e: DriverError) -> Self {
        Self::Driver(e)
    }
}

impl From<RunMigrationsError> for RepoError {
    fn from(e: RunMigrationsError) -> Self {
        Self::Migration(e)
    }
}

pub struct AsyncPool {
    pool: r2d2::Pool<ConnectionManager<SqliteConnection>>,
}

impl AsyncPool {
    pub fn new(path: &str) -> Self {
        let p = Path::new(path);

        let manager = ConnectionManager::new(p.to_str().expect("invalid data path"));

        let pool = r2d2::Pool::builder()
            .max_size(1) // sqlite does not support multiple writers.
            .build(manager)
            .expect("failed to initiate the pool");

        Self { pool }
    }

    /// Provide the migrations within the application so that it can be called
    /// on startup (or for tests).
    pub async fn run_migrations(&self) -> Result<(), RepoError> {
        let conn = self.get_conn().await?;

        embedded_migrations::run(&conn)?;

        Ok(())
    }

    pub async fn get_conn(&self) -> Result<Connection, RepoError> {
        tokio::task::block_in_place(|| self.pool.get().map_err(RepoError::from))
    }

    pub async fn exec<F, T>(&self, stmt: F) -> Result<T, RepoError>
    where
        F: FnOnce(Connection) -> QueryResult<T> + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::block_in_place(|| {
            let conn = self.pool.get().map_err(RepoError::from)?;

            stmt(conn).map_err(RepoError::from)
        })
    }
}

#[async_trait]
pub trait AccountRepository {
    async fn get_account(&self, addr: &str) -> Result<Account, RepoError>;
}
