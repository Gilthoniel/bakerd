use super::{AsyncPool, Result};
use crate::model::Block;
use crate::schema::blocks::dsl::*;
use diesel::prelude::*;
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
}

/// A repository to store the blocks observed by the application.
#[async_trait]
pub trait BlockRepository {
    async fn get_last_block(&self) -> Result<Block>;

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
        Self { pool }
    }
}

#[async_trait]
impl BlockRepository for SqliteBlockRepository {
    async fn get_last_block(&self) -> Result<Block> {
        let record: models::Block = self
            .pool
            .exec(|mut conn| blocks.order_by(height.desc()).first(&mut conn))
            .await?;

        Ok(Block::from(record))
    }

    async fn store(&self, block: models::NewBlock) -> Result<()> {
        self.pool
            .exec(move |mut conn| {
                diesel::insert_into(blocks)
                    .values(&block)
                    .execute(&mut conn)
            })
            .await?;

        Ok(())
    }

    async fn garbage_collect(&self, below_height: i64) -> Result<()> {
        self.pool
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
mockall::mock! {
    pub BlockRepository {
        pub fn get_last_block(&self) -> Result<Block>;
        pub fn store(&self, block: models::NewBlock) -> Result<()>;
        pub fn garbage_collect(&self, below_height: i64) -> Result<()>;
    }
}

#[cfg(test)]
#[async_trait]
impl BlockRepository for MockBlockRepository {
    async fn get_last_block(&self) -> Result<Block> {
        self.get_last_block()
    }

    async fn store(&self, block: models::NewBlock) -> Result<()> {
        self.store(block)
    }

    async fn garbage_collect(&self, below_height: i64) -> Result<()> {
        self.garbage_collect(below_height)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repository::{AsyncPool, RepositoryError};
    use diesel::result::Error;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_store_and_get_block() {
        let pool = AsyncPool::new(":memory:");
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
    async fn test_garbage_collect() {
        let pool = AsyncPool::new(":memory:");
        pool.run_migrations().await.unwrap();

        let repository = SqliteBlockRepository::new(pool);

        assert!(matches!(repository.garbage_collect(2840312).await, Ok(_)));

        assert!(matches!(
            repository.get_last_block().await,
            Err(e) if matches!(&e, RepositoryError::Driver(d) if matches!(d, Error::NotFound)),
        ));
    }
}
