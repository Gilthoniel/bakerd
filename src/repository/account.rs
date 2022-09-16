use super::{AsyncPool, Result};
use crate::model::{Account, Reward};
use crate::schema::account_rewards::dsl as reward_dsl;
use crate::schema::accounts::dsl::*;
use crate::schema::accounts::table;
use diesel::prelude::*;
use std::sync::Arc;

pub use models::{AccountFilter, NewAccount, NewReward, RewardKind};

mod models {
  use crate::model;
  use crate::schema::account_rewards;
  use crate::schema::accounts;
  use diesel::backend;
  use diesel::deserialize as de;
  use diesel::serialize as se;
  use diesel::sql_types::Text;
  use diesel::sqlite::Sqlite;
  use rust_decimal::Decimal;
  use std::str::FromStr;

  const REWARD_KIND_BAKER: &str = "kind_baker";
  const REWARD_KIND_TRANSACTION_FEE: &str = "kind_transaction_fee";

  #[derive(AsExpression, FromSqlRow, Debug)]
  #[diesel(sql_type = Text)]
  pub struct BigFloat(pub Decimal);

  impl From<Decimal> for BigFloat {
    fn from(v: Decimal) -> Self {
      Self(v)
    }
  }

  impl se::ToSql<Text, Sqlite> for BigFloat {
    fn to_sql(&self, out: &mut se::Output<Sqlite>) -> se::Result {
      let e = self.0.to_string();
      out.set_value(e);
      Ok(se::IsNull::No)
    }
  }

  impl de::FromSql<Text, Sqlite> for BigFloat {
    fn from_sql(value: backend::RawValue<Sqlite>) -> de::Result<Self> {
      let s = <String as de::FromSql<Text, Sqlite>>::from_sql(value)?;
      let res = Decimal::from_str(&s)?;
      Ok(BigFloat(res))
    }
  }

  /// Record of an account state on the blockchain.
  #[derive(Queryable)]
  pub struct Account {
    pub id: i32,
    pub address: String,
    pub lottery_power: f64,
    pub balance: BigFloat,
    pub stake: BigFloat,
    pub pending_update: bool,
  }

  #[derive(Default)]
  pub struct AccountFilter<'a> {
    pub addresses: Option<Vec<&'a str>>,
  }

  impl <'a> AccountFilter<'a> {
    /// It adds the list of addresses to the filter. Previous values will be overwritten.
    pub fn set_addresses(&mut self, addresses: &'a [impl AsRef<str>]) {
      self.addresses = Some(addresses.iter().map(AsRef::as_ref).collect());
    }
  }

  impl From<Account> for model::Account {
    /// It creates an account from a record of the storage layer.
    fn from(record: Account) -> Self {
      Self::new(
        record.id,
        &record.address,
        record.balance.0,
        record.stake.0,
        record.lottery_power,
      )
    }
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
    pub balance: BigFloat,
    pub stake: BigFloat,
    pub lottery_power: f64,
    pub pending_update: bool,
  }

  impl NewAccount {
    pub fn new(addr: &str, pending_update: bool) -> Self {
      Self {
        address: addr.into(),
        balance: BigFloat(Decimal::ZERO),
        stake: BigFloat(Decimal::ZERO),
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
    pub epoch_ms: i64,
    pub kind: RewardKind,
    pub amount: BigFloat,
  }

  impl From<Reward> for model::Reward {
    fn from(record: Reward) -> Self {
      Self::new(
        record.id,
        record.account_id,
        &record.block_hash,
        record.amount.0,
        record.epoch_ms,
        record.kind.into(),
      )
    }
  }

  // A enumeration of the possible reward kinds.
  #[derive(AsExpression, FromSqlRow, Debug)]
  #[diesel(sql_type = Text)]
  pub enum RewardKind {
    Baker,
    TransactionFee,
  }

  impl From<RewardKind> for model::RewardKind {
    /// It converts an SQL reward kind into the model one.
    fn from(kind: RewardKind) -> Self {
      match kind {
        RewardKind::Baker => Self::Baker,
        RewardKind::TransactionFee => Self::TransactionFee,
      }
    }
  }

  impl se::ToSql<Text, Sqlite> for RewardKind {
    fn to_sql(&self, out: &mut se::Output<Sqlite>) -> se::Result {
      let e = match self {
        Self::Baker => REWARD_KIND_BAKER,
        Self::TransactionFee => REWARD_KIND_TRANSACTION_FEE,
      };

      <str as se::ToSql<Text, Sqlite>>::to_sql(e, out)
    }
  }

  impl de::FromSql<Text, Sqlite> for RewardKind {
    fn from_sql(value: backend::RawValue<Sqlite>) -> de::Result<Self> {
      match <String as de::FromSql<Text, Sqlite>>::from_sql(value)?.as_str() {
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
    pub amount: BigFloat,
    pub epoch_ms: i64,
    pub kind: RewardKind,
  }
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait AccountRepository {
  /// It returns the account associated to the address if it exists.
  async fn get_account(&self, account: i32) -> Result<Account>;

  /// It returns the list of accounts associated with the addresses.
  async fn get_accounts<'a>(&self, filter: AccountFilter<'a>) -> Result<Vec<Account>>;

  /// It creates or updates an existing account using the address as the
  /// identifier.
  async fn set_account(&self, account: NewAccount) -> Result<Account>;

  /// It returns the list of rewards known for an account using the address to
  /// identity it.
  async fn get_rewards(&self, account: &Account) -> Result<Vec<Reward>>;

  /// It creates an account reward if it does not exist already. The reward is
  /// identified by the account, the block and its kind.
  async fn set_reward(&self, reward: NewReward) -> Result<()>;

  /// It sets the given either into pending for update, or inversely switch them off.
  async fn set_for_update(&self, addrs: Vec<String>, pending: bool) -> Result<()>;

  /// It returns the list of accounts that requires an update of their balances and lottery power.
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
  async fn get_account(&self, account: i32) -> Result<Account> {
    let record: models::Account = self
      .pool
      .exec(|mut conn| accounts.filter(id.eq(account)).first(&mut conn))
      .await?;

    Ok(Account::new(
      record.id,
      &record.address,
      record.balance.0,
      record.stake.0,
      record.lottery_power,
    ))
  }

  /// It returns the list of accounts associated with the addresses.
  async fn get_accounts<'a>(&self, filter: AccountFilter<'a>) -> Result<Vec<Account>> {
    let records: Vec<models::Account> = self
      .pool
      .exec(|mut conn| {
        let mut query = table.into_boxed();
        if let Some(addresses) = filter.addresses {
          query = query.filter(address.eq_any(addresses))
        }

        query.load(&mut conn)
      })
      .await?;

    Ok(records.into_iter().map(Account::from).collect())
  }

  /// It creates or updates an existing account using the address as the identifier.
  async fn set_account(&self, account: NewAccount) -> Result<Account> {
    let res: models::Account = self
      .pool
      .exec(move |mut conn| {
        conn.transaction(|tx| {
          diesel::insert_into(accounts)
            .values(&account)
            .on_conflict(address)
            .do_update()
            .set(&account)
            .execute(tx)?;

          accounts.filter(address.eq(account.address)).first(tx)
        })
      })
      .await?;

    Ok(Account::from(res))
  }

  /// It returns the list of rewards known for an account using the address to identity it.
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

  /// It creates an account reward if it does not exist already. The reward is identified by the
  /// account, the block and its kind.
  async fn set_reward(&self, reward: NewReward) -> Result<()> {
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

  /// It sets the given either into pending for update, or inversely switch them off.
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

  /// It returns the list of accounts that requires an update of their balances and lottery power.
  async fn get_for_update(&self) -> Result<Vec<Account>> {
    let res: Vec<models::Account> = self
      .pool
      .exec(|mut conn| accounts.filter(pending_update.eq(true)).load(&mut conn))
      .await?;

    Ok(res.into_iter().map(Account::from).collect())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::repository::{AsyncPool, RepositoryError};
  use rust_decimal_macros::dec;

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_account() {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let account = NewAccount {
      address: ":address:".into(),
      balance: dec!(250.2).into(),
      stake: dec!(50).into(),
      lottery_power: 0.0123,
      pending_update: false,
    };

    let repository = SqliteAccountRepository::new(pool);

    // 1. Create an account.
    assert!(matches!(repository.set_account(account).await, Ok(_)),);

    // 2. Get the account.
    let res = repository.get_account(1).await.unwrap();

    assert_eq!(1, res.get_id());
    assert_eq!(":address:", res.get_address());
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_account_failure() {
    let pool = AsyncPool::open(":memory:").unwrap();

    let repository = SqliteAccountRepository::new(pool);

    let res = repository.get_account(1).await;

    assert!(matches!(res, Err(RepositoryError::Faillable(_))));
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_set_account_failure() {
    let pool = AsyncPool::open(":memory:").unwrap();

    let repository = SqliteAccountRepository::new(pool);

    let account = NewAccount {
      address: ":address:".into(),
      balance: dec!(250).into(),
      stake: dec!(50).into(),
      lottery_power: 0.096,
      pending_update: false,
    };

    let res = repository.set_account(account).await;

    assert!(matches!(res, Err(RepositoryError::Faillable(_))));
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_accounts() -> Result<()> {
    let pool = AsyncPool::open(":memory:")?;

    pool.run_migrations().await?;

    let repository = SqliteAccountRepository::new(pool);

    repository.set_account(NewAccount::new(":address-1:", false)).await?;

    repository.set_account(NewAccount::new(":address-2:", true)).await?;

    let filter = AccountFilter {
      addresses: Some(vec![":address-1:", ":address-2:"]),
    };

    let res = repository.get_accounts(filter).await?;

    assert_eq!(res.len(), 2);

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_accounts_failure() -> Result<()> {
    let pool = AsyncPool::open(":memory:")?;

    let repository = SqliteAccountRepository::new(pool);

    let res = repository.get_accounts(AccountFilter::default()).await;

    assert!(matches!(res, Err(RepositoryError::Faillable(_))));

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_rewards() {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqliteAccountRepository::new(pool);

    repository
      .set_account(NewAccount::new(":address:", false))
      .await
      .unwrap();

    let account = repository.get_account(1).await.unwrap();

    repository
      .set_reward(NewReward {
        account_id: account.get_id(),
        block_hash: ":hash:".to_string(),
        amount: dec!(125).into(),
        epoch_ms: 0,
        kind: RewardKind::Baker,
      })
      .await
      .unwrap();

    repository
      .set_reward(NewReward {
        account_id: account.get_id(),
        block_hash: ":hash:".to_string(),
        amount: dec!(525).into(),
        epoch_ms: 0,
        kind: RewardKind::TransactionFee,
      })
      .await
      .unwrap();

    let res = repository.get_rewards(&account).await;

    assert!(matches!(res, Ok(rewards) if rewards.len() == 2));
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_rewards_failure() {
    let pool = AsyncPool::open(":memory:").unwrap();

    let repository = SqliteAccountRepository::new(pool);

    let account = Account::new(1, "address", dec!(0), dec!(0), 0.0);

    let res = repository.get_rewards(&account).await;

    assert!(matches!(res, Err(RepositoryError::Faillable(_))));
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_set_reward_failure() {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqliteAccountRepository::new(pool);

    let res = repository
      .set_reward(NewReward {
        account_id: 1,
        block_hash: ":hash:".to_string(),
        amount: dec!(525).into(),
        epoch_ms: 0,
        kind: RewardKind::TransactionFee,
      })
      .await;

    assert!(matches!(res, Err(RepositoryError::Faillable(_))), "value: {:?}", res);
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_set_pending() -> Result<()> {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqliteAccountRepository::new(pool);

    repository.set_account(NewAccount::new(":address-1:", true)).await?;

    repository.set_account(NewAccount::new(":address-2:", false)).await?;

    repository.set_account(NewAccount::new(":address-3:", false)).await?;

    repository
      .set_for_update(vec![":address-1:".into(), ":address-2:".into()], true)
      .await?;

    let res = repository.get_for_update().await?;

    assert_eq!(2, res.len());

    Ok(())
  }

  /// It tests that the function properly returns an error when no migration has run.
  #[tokio::test(flavor = "multi_thread")]
  async fn test_set_for_update_failure() {
    let pool = AsyncPool::open(":memory:").unwrap();

    let repository = SqliteAccountRepository::new(pool);

    let res = repository.set_for_update(vec![], true).await;

    assert!(matches!(res, Err(RepositoryError::Faillable(_))));
  }

  /// It tests that the function properly returns an error when no migration has run.
  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_for_update_failure() {
    let pool = AsyncPool::open(":memory:").unwrap();

    let repository = SqliteAccountRepository::new(pool);

    let res = repository.get_for_update().await;

    assert!(matches!(res, Err(RepositoryError::Faillable(_))));
  }
}
