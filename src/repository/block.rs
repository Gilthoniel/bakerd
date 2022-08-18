use super::{AsyncPool, StorageError};
use crate::model::Block;
use crate::schema::blocks::dsl::*;
use chrono::{DateTime, Utc};
use diesel::prelude::*;

pub use records::NewBlock;

pub mod records {
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
    async fn get_last_block(&self) -> Result<Block, StorageError>;

    async fn store(&self, block: NewBlock) -> Result<(), StorageError>;

    async fn garbage_collect(&self, before: DateTime<Utc>) -> Result<(), StorageError>;
}

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
    async fn get_last_block(&self) -> Result<Block, StorageError> {
        let record: records::Block = self
            .pool
            .exec(|mut conn| blocks.order_by(height.desc()).first(&mut conn))
            .await?;

        Ok(Block::from(record))
    }

    async fn store(&self, block: NewBlock) -> Result<(), StorageError> {
        self.pool
            .exec(move |mut conn| {
                diesel::insert_into(blocks)
                    .values(&block)
                    .execute(&mut conn)
            })
            .await?;

        Ok(())
    }

    async fn garbage_collect(&self, before: DateTime<Utc>) -> Result<(), StorageError> {
        let before_ms = before.timestamp_millis();

        self.pool
            .exec(move |mut conn| {
                diesel::delete(blocks)
                    .filter(slot_time_ms.le(before_ms))
                    .execute(&mut conn)
            })
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repository::AsyncPool;
    use chrono::TimeZone;
    use diesel::result::Error;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_store_and_get_block() {
        let pool = AsyncPool::new(":memory:");
        pool.run_migrations().await.unwrap();

        let repository = SqliteBlockRepository::new(pool);

        let new_block = NewBlock {
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

        let before = Utc.timestamp_millis(1651978740000);

        assert!(matches!(repository.garbage_collect(before).await, Ok(_)));

        assert!(matches!(
            repository.get_last_block().await,
            Err(e) if matches!(&e, StorageError::Driver(d) if matches!(d, Error::NotFound)),
        ));
    }
}