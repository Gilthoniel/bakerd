use async_trait::async_trait;
use diesel::prelude::*;
use std::sync::Arc;

use super::{AccountRepository, AsyncPool, RepoError};
use crate::model::Account;
use crate::schema::accounts::dsl::*;

pub type DynAccountRepository = Arc<dyn AccountRepository + Send + Sync>;

/// Record of an account state on the blockchain.
#[derive(Queryable)]
pub struct AccountRecord {
    pub id: i32,
    pub address: String,
    pub available_amount: String,
    pub staked_amount: String,
    pub lottery_power: f64,
}

/// Provide storage API to read and write accounts.
pub struct SqliteAccountRepository {
    pool: AsyncPool,
}

impl SqliteAccountRepository {
    /// It creates a new account repository using the given connection pool.
    pub fn new(pool: AsyncPool) -> DynAccountRepository {
        Arc::new(Self { pool })
    }
}

#[async_trait]
impl AccountRepository for SqliteAccountRepository {
    /// It returns the account with the given address if it exists.
    async fn get_account(&self, addr: &str) -> Result<Account, RepoError> {
        let addr = addr.to_string();

        let record: AccountRecord = self
            .pool
            .exec(|conn| accounts.filter(address.eq(addr)).first(&conn))
            .await?;

        Ok(Account::from(record))
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repository::AsyncPool;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_account() {
        let pool = AsyncPool::new(":memory:");
        pool.run_migrations().await.unwrap();

        // Create an account for the test.
        pool
            .exec(|conn| diesel::insert_into(accounts).values(address.eq("some-address")).execute(&conn))
            .await
            .unwrap();

        let repo = SqliteAccountRepository::new(pool);

        let res = repo.get_account("some-address").await.unwrap();
        assert_eq!(Account::new("some-address"), res);
    }
}
