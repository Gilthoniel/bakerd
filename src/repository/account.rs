use diesel::prelude::*;
use std::sync::Arc;

use super::{AccountRepository, AsyncPool, StorageError};
use crate::model::Account;
use crate::schema::accounts::dsl::*;

pub type DynAccountRepository = Arc<dyn AccountRepository + Send + Sync>;

mod records {
    use crate::model;
    use crate::schema::accounts;

    /// Record of an account state on the blockchain.
    #[derive(Queryable)]
    pub struct Account {
        pub id: i32,
        pub address: String,
        pub available_amount: String,
        pub staked_amount: String,
        pub lottery_power: f64,
    }

    impl From<Account> for model::Account {
        /// It creates an account from a record of the storage layer.
        fn from(record: Account) -> Self {
            Self::new(&record.address)
        }
    }

    #[derive(Insertable, AsChangeset)]
    #[diesel(table_name = accounts)]
    pub struct NewAccount {
        pub address: String,
        pub available_amount: String,
        pub staked_amount: String,
        pub lottery_power: f64,
    }
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
    async fn set_account(&self, account: &Account) -> Result<(), StorageError> {
        let record = records::NewAccount {
            address: account.get_address().into(),
            available_amount: "0".into(),
            staked_amount: "0".into(),
            lottery_power: 0.0,
        };

        self.pool
            .exec(move |mut conn| {
                diesel::insert_into(accounts)
                    .values(&record)
                    .on_conflict(address)
                    .do_update()
                    .set(&record)
                    .execute(&mut conn)
            })
            .await?;

        Ok(())
    }

    /// It returns the account with the given address if it exists.
    async fn get_account(&self, addr: &str) -> Result<Account, StorageError> {
        let addr = addr.to_string();

        let record: records::Account = self
            .pool
            .exec(|mut conn| accounts.filter(address.eq(addr)).first(&mut conn))
            .await?;

        Ok(Account::from(record))
    }
}

#[cfg(test)]
mockall::mock! {
  pub AccountRepository {
      pub fn set_account(&self, account: &Account) -> Result<(), StorageError>;

      pub fn get_account(&self, addr: &str) -> Result<Account, StorageError>;
  }
}

#[cfg(test)]
#[async_trait]
impl AccountRepository for MockAccountRepository {
    async fn set_account(&self, account: &Account) -> Result<(), StorageError> {
        self.set_account(account)
    }

    async fn get_account(&self, addr: &str) -> Result<Account, StorageError> {
        self.get_account(addr)
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

        let account = Account::new("some-address");

        let repo = SqliteAccountRepository::new(pool);

        // 1. Create an account.
        assert!(matches!(repo.set_account(&account).await, Ok(_)),);

        // 2. Get the account.
        let res = repo.get_account(account.get_address()).await.unwrap();
        assert_eq!(account, res);
    }
}
