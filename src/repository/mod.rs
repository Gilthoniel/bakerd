mod account;
mod block;
mod price;
mod status;

use diesel::r2d2::ConnectionManager;
use diesel::result::Error as DriverError;
use diesel::{QueryResult, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
use std::fmt;
use std::path::Path;

pub use self::{account::*, block::*, price::*, status::*};

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

/// A result of an exeuction of a repository function.
pub type Result<T> = std::result::Result<T, RepositoryError>;

/// An error produced by the repositories in different scenarios:
///   - Fallback error for unexpect issues coming from the database
///   - NotFound error for unknown resources
#[derive(Debug)]
pub enum RepositoryError {
    Faillable(Box<dyn std::error::Error>),
    NotFound,
}

/// It implements the standard error trait.
impl std::error::Error for RepositoryError {}

/// It implements the standard trait to nicely display the error when logged.
impl fmt::Display for RepositoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "resource not found"),
            Self::Faillable(e) => write!(f, "repository error: {}", e),
        }
    }
}

impl From<PoolError> for RepositoryError {
    // It transforms a pool error into a repository error by creating the most
    // generic repository error.
    fn from(e: PoolError) -> Self {
        match e {
            PoolError::Driver(e) => RepositoryError::Faillable(Box::new(e)),
            PoolError::Pool(e) => RepositoryError::Faillable(Box::new(e)),
            PoolError::Migration(e) => RepositoryError::Faillable(e),
        }
    }
}

/// An error that the pool can cause. It concerns either the database engine, or
/// more simply the connection to the database.
#[derive(Debug)]
pub enum PoolError {
    Driver(DriverError),
    Pool(r2d2::Error),
    Migration(Box<dyn std::error::Error>),
}

impl fmt::Display for PoolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Driver(e) => write!(f, "driver error: {}", e),
            Self::Pool(e) => write!(f, "pool error: {}", e),
            Self::Migration(e) => write!(f, "migration error: {}", e),
        }
    }
}

/// It implements the standard error trait for the pool error.
impl std::error::Error for PoolError {}

/// A result of a call to a pool function that results either in a result or a
/// pool error.
type PoolResult<T> = std::result::Result<T, PoolError>;

/// A asynchronous support of the blocking database pool.
#[derive(Clone)]
pub struct AsyncPool {
    pool: r2d2::Pool<ConnectionManager<SqliteConnection>>,
}

impl AsyncPool {
    /// It creates a new asynchronous pool using the path to open the file
    /// database, or to create an in-memory database using `:memory:`.
    pub fn new(path: &str) -> Self {
        let p = Path::new(path);

        // TODO: remove unwrap.
        let manager = ConnectionManager::new(p.to_str().expect("invalid data path"));

        let pool = r2d2::Pool::builder()
            .max_size(1) // sqlite does not support multiple writers.
            .build(manager)
            .expect("failed to initiate the pool");

        Self { pool }
    }

    /// Provide the migrations within the application so that it can be called
    /// on startup (or for tests).
    pub async fn run_migrations(&self) -> PoolResult<()> {
        let mut conn = self.get_conn().await?;

        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|e| PoolError::Migration(e))?;

        Ok(())
    }

    /// It returns a pooled connection.
    pub async fn get_conn(&self) -> PoolResult<Connection> {
        tokio::task::block_in_place(|| self.pool.get().map_err(|e| PoolError::Pool(e)))
    }

    /// It takes a list of statements to execute on a database connection.
    pub async fn exec<F, T>(&self, stmt: F) -> PoolResult<T>
    where
        F: FnOnce(Connection) -> QueryResult<T> + Send + 'static,
        T: Send + 'static,
    {
        tokio::task::block_in_place(|| {
            let conn = self.pool.get().map_err(|e| PoolError::Pool(e))?;

            stmt(conn).map_err(|e| PoolError::Driver(e))
        })
    }
}
