use super::{AsyncPool, PoolError, RepositoryError, Result};
use crate::model::Block;
use crate::schema::blocks::dsl::*;
use crate::schema::blocks::table;
use diesel::prelude::*;
use diesel::result::Error;
use std::sync::Arc;

pub mod models {
  use crate::schema::blocks;

  #[derive(Queryable)]
  pub struct Block {
    pub id: i32,
    pub height: i64,
    pub hash: String,
    pub slot_time_ms: i64,
    pub baker: i64,
  }

  #[derive(Insertable)]
  #[diesel(table_name = blocks)]
  pub struct NewBlock {
    pub height: i64,
    pub hash: String,
    pub slot_time_ms: i64,
    pub baker: i64,
  }

  pub struct BlockFilter {
    pub baker: Option<i64>,
    pub since_ms: Option<i64>,
  }
}

/// A repository to store the blocks observed by the application.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait BlockRepository {
  async fn get_last_block(&self) -> Result<Block>;

  async fn get_all(&self, filter: models::BlockFilter) -> Result<Vec<Block>>;

  async fn store(&self, block: models::NewBlock) -> Result<()>;

  /// It deletes the block with a height below the given value.
  async fn garbage_collect(&self, below_height: i64) -> Result<()>;
}

pub type DynBlockRepository = Arc<dyn BlockRepository + Sync + Send>;

/// A repository supported by SQLite to store the blocks observed by the
/// application.
pub struct SqliteBlockRepository {
  pool: AsyncPool,
}

impl SqliteBlockRepository {
  /// It creates a new repository with connections managed by the pool.
  pub fn new(pool: AsyncPool) -> Self {
    Self {
      pool,
    }
  }
}

#[async_trait]
impl BlockRepository for SqliteBlockRepository {
  async fn get_last_block(&self) -> Result<Block> {
    let record: models::Block = self
      .pool
      .exec(|mut conn| blocks.order_by(height.desc()).first(&mut conn))
      .await
      .map_err(|e| match e {
        PoolError::Driver(Error::NotFound) => RepositoryError::NotFound,
        _ => RepositoryError::from(e),
      })?;

    Ok(Block::from(record))
  }

  async fn get_all(&self, filter: models::BlockFilter) -> Result<Vec<Block>> {
    let records: Vec<models::Block> = self
      .pool
      .exec(move |mut conn| {
        let mut query = table.into_boxed();

        if let Some(baker_id) = filter.baker {
          query = query.filter(baker.eq(baker_id));
        }
        if let Some(since_ms) = filter.since_ms {
          query = query.filter(slot_time_ms.ge(since_ms));
        }

        query.order_by(height.desc()).load(&mut conn)
      })
      .await?;

    let res = records.into_iter().map(Block::from).collect();

    Ok(res)
  }

  async fn store(&self, block: models::NewBlock) -> Result<()> {
    self
      .pool
      .exec(move |mut conn| diesel::insert_into(blocks).values(&block).execute(&mut conn))
      .await?;

    Ok(())
  }

  async fn garbage_collect(&self, below_height: i64) -> Result<()> {
    self
      .pool
      .exec(move |mut conn| {
        diesel::delete(blocks)
          .filter(height.lt(below_height))
          .execute(&mut conn)
      })
      .await?;

    Ok(())
  }
}

#[cfg(test)]
mod integration_tests {
  use super::*;
  use crate::repository::{AsyncPool, RepositoryError};

  #[tokio::test(flavor = "multi_thread")]
  async fn test_store_and_get_block() {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqliteBlockRepository::new(pool);

    let new_block = models::NewBlock {
      // Previous block is inserted by the migration.
      height: 2840312,
      hash: ":hash:".to_string(),
      slot_time_ms: 123,
      baker: 2,
    };

    assert!(matches!(repository.store(new_block).await, Ok(_)));

    let res = repository.get_last_block().await;

    assert!(matches!(res, Ok(block) if block.get_height() == 2840312));
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_all() {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqliteBlockRepository::new(pool);

    let new_blocks = vec![
      models::NewBlock {
        height: 1,
        hash: ":hash-block-1:".into(),
        slot_time_ms: 1000,
        baker: 42,
      },
      models::NewBlock {
        height: 2,
        hash: ":hash-block-2:".into(),
        slot_time_ms: 1200,
        baker: 43,
      },
      models::NewBlock {
        height: 3,
        hash: ":hash-block-3:".into(),
        slot_time_ms: 1500,
        baker: 42,
      },
    ];

    for block in new_blocks {
      assert!(matches!(repository.store(block).await, Ok(_)));
    }

    let res = repository
      .get_all(models::BlockFilter {
        baker: Some(42),
        since_ms: Some(1000),
      })
      .await;

    assert!(matches!(&res, Ok(bb) if bb.len() == 2), "wrong result: {:?}", res);

    let res = repository
      .get_all(models::BlockFilter {
        baker: None,
        since_ms: Some(1000),
      })
      .await;

    assert!(matches!(&res, Ok(bb) if bb.len() == 4), "wrong result: {:?}", res);
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_garbage_collect() {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqliteBlockRepository::new(pool);

    assert!(matches!(repository.garbage_collect(2840312).await, Ok(_)));

    assert!(matches!(
      repository.get_last_block().await,
      Err(RepositoryError::NotFound),
    ));
  }
}
