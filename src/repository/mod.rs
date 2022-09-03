mod account;
mod block;
mod price;
mod status;
mod user;

use diesel::connection::SimpleConnection;
use diesel::r2d2::{ConnectionManager, Error as R2d2Error};
use diesel::result::Error as DriverError;
use diesel::{QueryResult, SqliteConnection};
use diesel_migrations::{EmbeddedMigrations, MigrationHarness};
use r2d2::CustomizeConnection;
use std::fmt;
use std::time::Duration;

pub use self::{account::*, block::*, price::*, status::*, user::*};

pub mod models {
  pub use super::account::models::*;
  pub use super::block::models::*;
  pub use super::price::models::*;
  pub use super::status::models::*;
  pub use super::user::models::*;
}

/// A embedding of the migrations of the application to package them alongside
/// the binary.
const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

const ERROR_PREFIX: &str = "repository error";
const ERROR_NOT_FOUND: &str = "resource not found";

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
      Self::NotFound => write!(f, "{}", ERROR_NOT_FOUND),
      Self::Faillable(e) => write!(f, "{}: {}", ERROR_PREFIX, e),
    }
  }
}

impl From<PoolError> for RepositoryError {
  // It transforms a pool error into a repository error by creating the most
  // generic repository error.
  fn from(e: PoolError) -> Self {
    match e {
      PoolError::Driver(e) => RepositoryError::Faillable(Box::new(e)),
      PoolError::Faillable(e) => RepositoryError::Faillable(e),
    }
  }
}

/// An error that the pool can cause. It concerns either the database engine, or
/// more simply the connection to the database.
#[derive(Debug)]
pub enum PoolError {
  Driver(DriverError),
  Faillable(Box<dyn std::error::Error>),
}

impl From<r2d2::Error> for PoolError {
  fn from(e: r2d2::Error) -> Self {
    Self::Faillable(Box::new(e))
  }
}

impl From<DriverError> for PoolError {
  fn from(e: DriverError) -> Self {
    Self::Driver(e)
  }
}

impl fmt::Display for PoolError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Driver(e) => write!(f, "driver error: {}", e),
      Self::Faillable(e) => write!(f, "storage layer error: {}", e),
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
  pub fn open(path: &str) -> PoolResult<Self> {
    Self::open_with_timeout(path, Duration::from_secs(30))
  }

  fn open_with_timeout(path: &str, timeout: Duration) -> PoolResult<Self> {
    let manager = ConnectionManager::new(path);

    let pool = r2d2::Pool::builder()
      .connection_customizer(Box::new(ConnectionCustomizer {}))
      .max_size(1) // sqlite does not support multiple writers.
      .connection_timeout(timeout)
      .build(manager)?;

    Ok(Self {
      pool,
    })
  }

  /// Provide the migrations within the application so that it can be called
  /// on startup (or for tests).
  pub async fn run_migrations(&self) -> PoolResult<()> {
    let mut conn = self.get_conn().await?;

    conn
      .run_pending_migrations(MIGRATIONS)
      .map_err(|e| PoolError::Faillable(e))?;

    Ok(())
  }

  /// It returns a pooled connection.
  pub async fn get_conn(&self) -> PoolResult<Connection> {
    Ok(tokio::task::block_in_place(|| self.pool.get())?)
  }

  /// It takes a list of statements to execute on a database connection.
  pub async fn exec<F, T>(&self, stmt: F) -> PoolResult<T>
  where
    F: FnOnce(Connection) -> QueryResult<T> + Send + 'static,
    T: Send + 'static,
  {
    tokio::task::block_in_place(|| {
      let conn = self.pool.get()?;

      Ok(stmt(conn)?)
    })
  }
}

/// A connection customizer to enable some features of SQLite.
/// - Foreign key constraint check.
#[derive(Debug)]
struct ConnectionCustomizer;

impl<C: SimpleConnection> CustomizeConnection<C, R2d2Error> for ConnectionCustomizer {
  /// It executes the statements after a connection is established. It then insures some features
  /// are enabled for any connections.
  fn on_acquire(&self, conn: &mut C) -> std::result::Result<(), R2d2Error> {
    conn
      .batch_execute("PRAGMA foreign_keys = true;")
      .map_err(|e| R2d2Error::QueryError(e))
  }

  fn on_release(&self, _conn: C) {}
}

#[cfg(test)]
mod tests {
  use super::*;
  use mockall::predicate::*;

  #[derive(Debug)]
  struct FakeError;

  impl fmt::Display for FakeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
      write!(f, "fake")
    }
  }

  impl std::error::Error for FakeError {}

  mockall::mock! {
    Connection {}
    impl SimpleConnection for Connection {
      fn batch_execute(&mut self, query: &str) -> QueryResult<()>;
    }
  }

  #[test]
  fn test_repository_error() {
    assert_eq!(ERROR_NOT_FOUND.to_string(), format!("{}", RepositoryError::NotFound));
    assert_eq!(
      format!("{}: fake", ERROR_PREFIX),
      format!("{}", RepositoryError::Faillable(Box::new(FakeError {})))
    );
  }

  #[test]
  fn test_pool_error() {
    assert_eq!(
      format!("driver error: {}", DriverError::NotFound),
      format!("{}", PoolError::from(DriverError::NotFound)),
    );
    assert_eq!(
      format!("storage layer error: fake"),
      format!("{}", PoolError::Faillable(Box::new(FakeError {}))),
    );
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_bad_connection() {
    let pool = AsyncPool::open_with_timeout(":memory:", Duration::from_millis(100)).unwrap();

    let _initial = pool.get_conn().await.unwrap();

    // Second request for a connection will fail as we have maximum one.
    let res = pool.get_conn().await;

    assert!(matches!(
      res.map_err(RepositoryError::from),
      Err(RepositoryError::Faillable(_))
    ));
  }

  #[test]
  fn test_connection_customizer() {
    let mut conn = MockConnection::new();

    conn
      .expect_batch_execute()
      .with(eq("PRAGMA foreign_keys = true;"))
      .times(1)
      .returning(|_| Ok(()));

    let customizer = ConnectionCustomizer {};

    // expect the pragma to be executed.
    let res = customizer.on_acquire(&mut conn);
    assert!(matches!(res, Ok(_)));

    // make sure nothing more is executed.
    customizer.on_release(conn);
  }

  #[test]
  fn test_connection_customizer_failure() {
    let mut conn = MockConnection::new();

    conn
      .expect_batch_execute()
      .with(eq("PRAGMA foreign_keys = true;"))
      .times(1)
      .returning(|_| Err(DriverError::AlreadyInTransaction));

    let customizer = ConnectionCustomizer {};

    // expect the pragma to be executed.
    let res = customizer.on_acquire(&mut conn);
    assert!(matches!(res, Err(R2d2Error::QueryError(_))));
  }
}
