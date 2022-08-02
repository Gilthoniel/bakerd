pub mod account;

use async_trait::async_trait;
use diesel::r2d2::ConnectionManager;
use diesel::result::Error as DriverError;
use diesel::{QueryResult, SqliteConnection};
use std::path::Path;

use crate::model::Account;

type Connection = r2d2::PooledConnection<ConnectionManager<SqliteConnection>>;

#[derive(Debug)]
pub enum RepoError {
    Pool(r2d2::Error),
    Driver(DriverError),
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

pub struct AsyncPool {
    pool: r2d2::Pool<ConnectionManager<SqliteConnection>>,
}

impl AsyncPool {
    pub fn new(dir: &str) -> Self {
        let p = Path::new(dir).join("data.db");

        let manager = ConnectionManager::new(p.to_str().expect("invalid data path"));

        let pool = r2d2::Pool::builder()
            .max_size(1) // sqlite does not support multiple writers.
            .build(manager)
            .expect("failed to initiate the pool");

        Self { pool }
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
