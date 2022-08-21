mod account;
mod block;
mod price;
mod status;

use axum::http::StatusCode;
use diesel::r2d2::ConnectionManager;
use diesel::result::Error as DriverError;
use diesel::{QueryResult, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
use std::path::Path;

pub use account::{AccountRepository, DynAccountRepository, SqliteAccountRepository};
pub use block::{BlockRepository, DynBlockRepository, SqliteBlockRepository};
pub use price::{DynPriceRepository, PriceRepository, SqlitePriceRepository};
pub use status::{DynStatusRepository, SqliteStatusRepository, StatusRepository};

#[cfg(test)]
pub use account::MockAccountRepository;

#[cfg(test)]
pub use block::MockBlockRepository;

#[cfg(test)]
pub use price::MockPriceRepository;

#[cfg(test)]
pub use status::MockStatusRepository;

/// Re-import of the records of the different repository.
pub mod models {
    pub use super::account::models::*;
    pub use super::block::models::*;
    pub use super::price::models::*;
    pub use super::status::models::*;
}

/// A embedding of the migrations of the application to package them alongside
/// the binary.
const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

/// An alias of the pooled connection from the r2d2 crate for SQLite.
type Connection = r2d2::PooledConnection<ConnectionManager<SqliteConnection>>;

pub type Result<T> = std::result::Result<T, RepositoryError>;

/// A generic error for the implementations of the different repositories.
#[derive(Debug)]
pub enum RepositoryError {
    Pool(r2d2::Error),
    Driver(DriverError),
    Migration(Box<dyn std::error::Error>),
}

impl RepositoryError {
    /// It returns a tuple of the HTTP code associated with the storage error as
    /// well as a human-readable message.
    pub fn status_code(&self) -> (StatusCode, &'static str) {
        match self {
            Self::Driver(e) if matches!(e, DriverError::NotFound) => {
                (StatusCode::NOT_FOUND, "resource does not exist")
            }
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "internal server error"),
        }
    }
}

impl From<r2d2::Error> for RepositoryError {
    fn from(e: r2d2::Error) -> Self {
        Self::Pool(e)
    }
}

impl From<DriverError> for RepositoryError {
    fn from(e: DriverError) -> Self {
        Self::Driver(e)
    }
}

#[derive(Clone)]
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
    pub async fn run_migrations(&self) -> Result<()> {
        let mut conn = self.get_conn().await?;

        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| RepositoryError::Migration(e))?;

        Ok(())
    }

    pub async fn get_conn(&self) -> Result<Connection> {
        tokio::task::block_in_place(|| self.pool.get().map_err(RepositoryError::from))
    }

    pub async fn exec<F, T>(&self, stmt: F) -> Result<T>
    where
        F: FnOnce(Connection) -> QueryResult<T> + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::block_in_place(|| {
            let conn = self.pool.get().map_err(RepositoryError::from)?;

            stmt(conn).map_err(RepositoryError::from)
        })
    }
}
