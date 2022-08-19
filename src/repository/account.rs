use diesel::prelude::*;

use super::{AsyncPool, StorageError};
use crate::model::{Account, Reward};
use crate::schema::account_rewards::dsl as reward_dsl;
use crate::schema::accounts::dsl::*;

pub use records::{NewAccount, NewReward, RewardKind};

pub mod records {
    use crate::schema::account_rewards;
    use crate::schema::accounts;
    use diesel::backend;
    use diesel::deserialize;
    use diesel::serialize;
    use diesel::sql_types::Text;
    use diesel::sqlite::Sqlite;

    const REWARD_KIND_BAKER: &str = "kind_baker";
    const REWARD_KIND_TRANSACTION_FEE: &str = "kind_transaction_fee";

    /// Record of an account state on the blockchain.
    #[derive(Queryable)]
    pub struct Account {
        pub id: i32,
        pub address: String,
        pub available_amount: String,
        pub staked_amount: String,
        pub lottery_power: f64,
    }

    #[derive(Insertable, AsChangeset)]
    #[diesel(table_name = accounts)]
    pub struct NewAccount {
        pub address: String,
        pub available_amount: String,
        pub staked_amount: String,
        pub lottery_power: f64,
    }

    #[derive(Queryable)]
    pub struct Reward {
        pub id: i32,
        pub account_id: i32,
        pub block_hash: String,
        pub amount: String,
        pub epoch_ms: i64,
        pub kind: RewardKind,
    }

    // A enumeration of the possible reward kinds.
    #[derive(AsExpression, FromSqlRow, Debug)]
    #[diesel(sql_type = Text)]
    pub enum RewardKind {
        Baker,
        TransactionFee,
    }

    impl serialize::ToSql<Text, Sqlite> for RewardKind {
        fn to_sql(&self, out: &mut serialize::Output<Sqlite>) -> serialize::Result {
            let e = match self {
                Self::Baker => REWARD_KIND_BAKER,
                Self::TransactionFee => REWARD_KIND_TRANSACTION_FEE,
            };

            <str as serialize::ToSql<Text, Sqlite>>::to_sql(e, out)
        }
    }

    impl deserialize::FromSql<Text, Sqlite> for RewardKind {
        fn from_sql(value: backend::RawValue<Sqlite>) -> deserialize::Result<Self> {
            match <String as deserialize::FromSql<Text, Sqlite>>::from_sql(value)?.as_str() {
                REWARD_KIND_BAKER => Ok(RewardKind::Baker),
                REWARD_KIND_TRANSACTION_FEE => Ok(RewardKind::TransactionFee),
                x => Err(format!("unrecognized value for enum: {}", x).into()),
            }
        }
    }

    #[derive(Insertable, AsChangeset)]
    #[diesel(table_name = account_rewards)]
    pub struct NewReward {
        pub account_id: i32,
        pub block_hash: String,
        pub amount: String,
        pub epoch_ms: i64,
        pub kind: RewardKind,
    }
}

#[async_trait]
pub trait AccountRepository {
    /// It returns the account associated to the address if it exists.
    async fn get_account(&self, addr: &str) -> Result<Account, StorageError>;

    /// It creates or updates an existing account using the address as the
    /// identifier.
    async fn set_account(&self, account: NewAccount) -> Result<(), StorageError>;

    /// It returns the list of rewards known for an account using the address to
    /// identity it.
    async fn get_rewards(&self, account: &Account) -> Result<Vec<Reward>, StorageError>;

    /// It creates an account reward if it does not exist already. The reward is
    /// identified by the account, the block and its kind.
    async fn set_reward(&self, reward: NewReward) -> Result<(), StorageError>;
}

/// Provide storage API to read and write accounts.
pub struct SqliteAccountRepository {
    pool: AsyncPool,
}

impl SqliteAccountRepository {
    /// It creates a new account repository using the given connection pool.
    pub fn new(pool: AsyncPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl AccountRepository for SqliteAccountRepository {
    /// It returns the account with the given address if it exists.
    async fn get_account(&self, addr: &str) -> Result<Account, StorageError> {
        let addr = addr.to_string();

        let record: records::Account = self
            .pool
            .exec(|mut conn| accounts.filter(address.eq(addr)).first(&mut conn))
            .await?;

        Ok(Account::from(record))
    }

    async fn set_account(&self, account: NewAccount) -> Result<(), StorageError> {
        self.pool
            .exec(move |mut conn| {
                diesel::insert_into(accounts)
                    .values(&account)
                    .on_conflict(address)
                    .do_update()
                    .set(&account)
                    .execute(&mut conn)
            })
            .await?;

        Ok(())
    }

    async fn get_rewards(&self, account: &Account) -> Result<Vec<Reward>, StorageError> {
        let account_id = account.get_id();

        let res: Vec<records::Reward> = self
            .pool
            .exec(move |mut conn| {
                reward_dsl::account_rewards
                    .filter(reward_dsl::account_id.eq(account_id))
                    .load(&mut conn)
            })
            .await?;

        Ok(res.into_iter().map(Reward::from).collect())
    }

    async fn set_reward(&self, reward: NewReward) -> Result<(), StorageError> {
        self.pool
            .exec(move |mut conn| {
                diesel::insert_into(reward_dsl::account_rewards)
                    .values(&reward)
                    .on_conflict((
                        reward_dsl::account_id,
                        reward_dsl::block_hash,
                        reward_dsl::kind,
                    ))
                    .do_nothing()
                    .execute(&mut conn)
            })
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mockall::mock! {
    pub AccountRepository {
        pub fn get_account(&self, addr: &str) -> Result<Account, StorageError>;
        pub fn set_account(&self, account: NewAccount) -> Result<(), StorageError>;
        pub fn get_rewards(&self, account: &Account) -> Result<Vec<Reward>, StorageError>;
        pub fn set_reward(&self, reward: NewReward) -> Result<(), StorageError>;
    }
}

#[cfg(test)]
#[async_trait]
impl AccountRepository for MockAccountRepository {
    async fn get_account(&self, addr: &str) -> Result<Account, StorageError> {
        self.get_account(addr)
    }

    async fn set_account(&self, account: NewAccount) -> Result<(), StorageError> {
        self.set_account(account)
    }

    async fn get_rewards(&self, account: &Account) -> Result<Vec<Reward>, StorageError> {
        self.get_rewards(account)
    }

    async fn set_reward(&self, reward: NewReward) -> Result<(), StorageError> {
        self.set_reward(reward)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repository::account::records::Account as AccountRecord;
    use crate::repository::AsyncPool;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_account() {
        let pool = AsyncPool::new(":memory:");
        pool.run_migrations().await.unwrap();

        let expect = AccountRecord {
            id: 1,
            address: ":address:".into(),
            available_amount: "250".into(),
            staked_amount: "50".into(),
            lottery_power: 0.096,
        };

        let account = NewAccount {
            address: expect.address.clone(),
            available_amount: expect.available_amount.clone(),
            staked_amount: expect.staked_amount.clone(),
            lottery_power: expect.lottery_power,
        };

        let repo = SqliteAccountRepository::new(pool);

        // 1. Create an account.
        assert!(matches!(repo.set_account(account).await, Ok(_)),);

        // 2. Get the account.
        let res = repo.get_account(":address:").await.unwrap();

        assert_eq!(Account::from(expect), res);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_rewards() {
        let pool = AsyncPool::new(":memory:");
        pool.run_migrations().await.unwrap();

        let repository = SqliteAccountRepository::new(pool);

        repository
            .set_account(NewAccount {
                address: ":address:".to_string(),
                available_amount: "0".to_string(),
                staked_amount: "0".to_string(),
                lottery_power: 0.0,
            })
            .await
            .unwrap();

        let account = repository.get_account(":address:").await.unwrap();

        repository
            .set_reward(NewReward {
                account_id: account.get_id(),
                block_hash: ":hash:".to_string(),
                amount: "0.125".to_string(),
                epoch_ms: 0,
                kind: RewardKind::Baker,
            })
            .await
            .unwrap();

        repository
            .set_reward(NewReward {
                account_id: account.get_id(),
                block_hash: ":hash:".to_string(),
                amount: "0.525".to_string(),
                epoch_ms: 0,
                kind: RewardKind::TransactionFee,
            })
            .await
            .unwrap();

        let res = repository.get_rewards(&account).await;

        assert!(matches!(res, Ok(rewards) if rewards.len() == 2));
    }
}
