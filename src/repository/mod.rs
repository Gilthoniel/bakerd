pub mod account;
pub mod price;

use axum::http::StatusCode;
use diesel::r2d2::ConnectionManager;
use diesel::result::Error as DriverError;
use diesel::{QueryResult, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
use std::path::Path;

use crate::model::{Account, Pair, Price};

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

type Connection = r2d2::PooledConnection<ConnectionManager<SqliteConnection>>;

#[derive(Debug)]
pub enum StorageError {
    Pool(r2d2::Error),
    Driver(DriverError),
    Migration(Box<dyn std::error::Error>),
}

impl StorageError {
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

impl From<r2d2::Error> for StorageError {
    fn from(e: r2d2::Error) -> Self {
        Self::Pool(e)
    }
}

impl From<DriverError> for StorageError {
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
    pub async fn run_migrations(&self) -> Result<(), StorageError> {
        let mut conn = self.get_conn().await?;

        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| StorageError::Migration(e))?;

        Ok(())
    }

    pub async fn get_conn(&self) -> Result<Connection, StorageError> {
        tokio::task::block_in_place(|| self.pool.get().map_err(StorageError::from))
    }

    pub async fn exec<F, T>(&self, stmt: F) -> Result<T, StorageError>
    where
        F: FnOnce(Connection) -> QueryResult<T> + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::block_in_place(|| {
            let conn = self.pool.get().map_err(StorageError::from)?;

            stmt(conn).map_err(StorageError::from)
        })
    }
}

#[async_trait]
pub trait AccountRepository {
    async fn get_account(&self, addr: &str) -> Result<Account, StorageError>;

    /// It creates or updates an existing account using the address as the
    /// identifier.
    async fn set_account(&self, account: &Account) -> Result<(), StorageError>;
}

/// A repository to set and get prices of pairs.
#[async_trait]
pub trait PriceRepository {
    /// It takes a pair and return the price if found in the storage.
    async fn get_price(&self, pair: &Pair) -> Result<Price, StorageError>;

    /// It takes a price and insert or update the price in the storage.
    async fn set_price(&self, price: &Price) -> Result<(), StorageError>;
}
