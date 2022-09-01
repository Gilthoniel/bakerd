use super::{AsyncPool, PoolError, RepositoryError, Result};
use crate::model::{Account, Reward};
use crate::schema::account_rewards::dsl as reward_dsl;
use crate::schema::accounts::dsl::*;
use diesel::prelude::*;
use diesel::result::Error;
use std::sync::Arc;

pub mod models {
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
    pub pending_update: bool,
  }

  #[derive(Identifiable)]
  #[diesel(table_name = accounts)]
  pub struct AccountID {
    pub id: i32,
  }

  #[derive(Insertable, AsChangeset)]
  #[diesel(table_name = accounts)]
  pub struct NewAccount {
    pub address: String,
    pub available_amount: String,
    pub staked_amount: String,
    pub lottery_power: f64,
    pub pending_update: bool,
  }

  impl NewAccount {
    pub fn new(addr: &str, pending_update: bool) -> Self {
      Self {
        address: addr.into(),
        available_amount: String::from("0"),
        staked_amount: String::from("0"),
        lottery_power: 0.0,
        pending_update,
      }
    }
  }

  #[derive(Queryable, Identifiable, Associations)]
  #[diesel(table_name = account_rewards, belongs_to(AccountID, foreign_key = account_id))]
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

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait AccountRepository {
  /// It returns the account associated to the address if it exists.
  async fn get_account(&self, addr: &str) -> Result<Account>;

  /// It returns the list of accounts associated with the addresses.
  async fn get_all(&self, addrs: Vec<String>) -> Result<Vec<Account>>;

  /// It creates or updates an existing account using the address as the
  /// identifier.
  async fn set_account(&self, account: models::NewAccount) -> Result<()>;

  /// It returns the list of rewards known for an account using the address to
  /// identity it.
  async fn get_rewards(&self, account: &Account) -> Result<Vec<Reward>>;

  /// It creates an account reward if it does not exist already. The reward is
  /// identified by the account, the block and its kind.
  async fn set_reward(&self, reward: models::NewReward) -> Result<()>;

  /// It switches the given accounts to a state such that their balance and lottery power will be
  /// updated.
  async fn set_for_update(&self, addrs: Vec<String>, pending: bool) -> Result<()>;

  /// It returns the list of accounts (max: 10) that requires an update of their balances and
  /// lottery power.
  async fn get_for_update(&self) -> Result<Vec<Account>>;
}

/// An alias of a singleton of an account repository shared in the application.
pub type DynAccountRepository = Arc<dyn AccountRepository + Send + Sync>;

/// Provide storage API to read and write accounts.
pub struct SqliteAccountRepository {
  pool: AsyncPool,
}

impl SqliteAccountRepository {
  /// It creates a new account repository using the given connection pool.
  pub fn new(pool: AsyncPool) -> Self {
    Self {
      pool,
    }
  }
}

#[async_trait]
impl AccountRepository for SqliteAccountRepository {
  /// It returns the account with the given address if it exists.
  async fn get_account(&self, addr: &str) -> Result<Account> {
    let addr = addr.to_string();

    let record: models::Account = self
      .pool
      .exec(|mut conn| accounts.filter(address.eq(addr)).first(&mut conn))
      .await
      .map_err(|e| match e {
        PoolError::Driver(Error::NotFound) => RepositoryError::NotFound,
        _ => RepositoryError::from(e),
      })?;

    Ok(Account::from(record))
  }

  /// It returns the list of accounts associated with the addresses.
  async fn get_all(&self, addrs: Vec<String>) -> Result<Vec<Account>> {
    let records: Vec<models::Account> = self
      .pool
      .exec(|mut conn| accounts.filter(address.eq_any(addrs)).load(&mut conn))
      .await?;

    Ok(records.into_iter().map(Account::from).collect())
  }

  async fn set_account(&self, account: models::NewAccount) -> Result<()> {
    self
      .pool
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

  async fn get_rewards(&self, account: &Account) -> Result<Vec<Reward>> {
    let account_id = models::AccountID {
      id: account.get_id(),
    };

    let res: Vec<models::Reward> = self
      .pool
      .exec(move |mut conn| models::Reward::belonging_to(&account_id).load(&mut conn))
      .await?;

    Ok(res.into_iter().map(Reward::from).collect())
  }

  async fn set_reward(&self, reward: models::NewReward) -> Result<()> {
    self
      .pool
      .exec(move |mut conn| {
        diesel::insert_into(reward_dsl::account_rewards)
          .values(&reward)
          .on_conflict((reward_dsl::account_id, reward_dsl::block_hash, reward_dsl::kind))
          .do_nothing()
          .execute(&mut conn)
      })
      .await?;

    Ok(())
  }

  async fn set_for_update(&self, addrs: Vec<String>, pending: bool) -> Result<()> {
    self
      .pool
      .exec(move |mut conn| {
        diesel::update(accounts.filter(address.eq_any(addrs)))
          .set(pending_update.eq(pending))
          .execute(&mut conn)
      })
      .await?;

    Ok(())
  }

  async fn get_for_update(&self) -> Result<Vec<Account>> {
    let res: Vec<models::Account> = self
      .pool
      .exec(|mut conn| accounts.filter(pending_update.eq(true)).load(&mut conn))
      .await?;

    Ok(res.into_iter().map(Account::from).collect())
  }
}

#[cfg(test)]
mod integration_tests {
  use super::*;
  use crate::repository::AsyncPool;

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_account() {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let expect = models::Account {
      id: 1,
      address: ":address:".into(),
      available_amount: "250".into(),
      staked_amount: "50".into(),
      lottery_power: 0.096,
      pending_update: false,
    };

    let account = models::NewAccount {
      address: expect.address.clone(),
      available_amount: expect.available_amount.clone(),
      staked_amount: expect.staked_amount.clone(),
      lottery_power: expect.lottery_power,
      pending_update: expect.pending_update,
    };

    let repository = SqliteAccountRepository::new(pool);

    // 1. Create an account.
    assert!(matches!(repository.set_account(account).await, Ok(_)),);

    // 2. Get the account.
    let res = repository.get_account(":address:").await.unwrap();

    assert_eq!(Account::from(expect), res);
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_all() -> Result<()> {
    let pool = AsyncPool::open(":memory:")?;

    pool.run_migrations().await?;

    let repository = SqliteAccountRepository::new(pool);

    repository
      .set_account(models::NewAccount::new(":address-1:", false))
      .await?;

    repository
      .set_account(models::NewAccount::new(":address-2:", true))
      .await?;

    let addresses = vec![":address-1:".to_string(), ":address-2:".to_string()];

    let res = repository.get_all(addresses).await?;

    assert_eq!(res.len(), 2);

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_rewards() {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqliteAccountRepository::new(pool);

    repository
      .set_account(models::NewAccount::new(":address:", false))
      .await
      .unwrap();

    let account = repository.get_account(":address:").await.unwrap();

    repository
      .set_reward(models::NewReward {
        account_id: account.get_id(),
        block_hash: ":hash:".to_string(),
        amount: "0.125".to_string(),
        epoch_ms: 0,
        kind: models::RewardKind::Baker,
      })
      .await
      .unwrap();

    repository
      .set_reward(models::NewReward {
        account_id: account.get_id(),
        block_hash: ":hash:".to_string(),
        amount: "0.525".to_string(),
        epoch_ms: 0,
        kind: models::RewardKind::TransactionFee,
      })
      .await
      .unwrap();

    let res = repository.get_rewards(&account).await;

    assert!(matches!(res, Ok(rewards) if rewards.len() == 2));
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_set_pending() -> Result<()> {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqliteAccountRepository::new(pool);

    repository
      .set_account(models::NewAccount::new(":address-1:", true))
      .await?;

    repository
      .set_account(models::NewAccount::new(":address-2:", false))
      .await?;

      repository
      .set_account(models::NewAccount::new(":address-3:", false))
      .await?;

    repository.set_for_update(vec![":address-1:".into(), ":address-2:".into()], true).await?;

    let res = repository.get_for_update().await?;

    assert_eq!(2, res.len());

    Ok(())
  }
}
