use crate::repository::models;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// An account on the Concordium blockchain. It is uniquely identified through
/// the address.
#[derive(PartialEq, Clone, Debug, Serialize)]
pub struct Account {
  id: i32,
  address: String,
  available_amount: Decimal,
  staked_amount: Decimal,
  lottery_power: f64,
}

impl Account {
  /// It returns the ID of the account in the storage layer.
  pub fn get_id(&self) -> i32 {
    return self.id;
  }
}

impl From<models::Account> for Account {
  /// It creates an account from a record of the storage layer.
  fn from(record: models::Account) -> Self {
    Self {
      id: record.id,
      address: record.address,
      available_amount: to_decimal(&record.available_amount),
      staked_amount: to_decimal(&record.staked_amount),
      lottery_power: record.lottery_power,
    }
  }
}

/// A enumeration of the reward kinds. It supports serialization into a human
/// readable string.
#[derive(Serialize, Debug)]
pub enum RewardKind {
  #[serde(rename = "kind_baker")]
  Baker,

  #[serde(rename = "kind_transaction_fee")]
  TransactionFee,
}

impl From<models::RewardKind> for RewardKind {
  /// It converts an SQL reward kind into the model one.
  fn from(kind: models::RewardKind) -> Self {
    match kind {
      models::RewardKind::Baker => Self::Baker,
      models::RewardKind::TransactionFee => Self::TransactionFee,
    }
  }
}

/// A reward of a baker which can be either a baker reward or the transaction
/// fees.
#[derive(Serialize, Debug)]
pub struct Reward {
  id: i32,
  account_id: i32,
  block_hash: String,
  amount: Decimal,
  epoch_ms: i64,
  kind: RewardKind,
}

impl From<models::Reward> for Reward {
  fn from(record: models::Reward) -> Self {
    Self {
      id: record.id,
      account_id: record.account_id,
      block_hash: record.block_hash,
      amount: to_decimal(&record.amount),
      epoch_ms: record.epoch_ms,
      kind: RewardKind::from(record.kind),
    }
  }
}

/// A unique combination of a base and a quote currency.
#[derive(PartialEq, Clone, Deserialize, Serialize, Debug)]
pub struct Pair(String, String);

impl Pair {
  pub fn base(&self) -> &str {
    &self.0
  }

  pub fn quote(&self) -> &str {
    &self.1
  }
}

impl From<(&str, &str)> for Pair {
  fn from((base, quote): (&str, &str)) -> Self {
    Self(base.to_string(), quote.to_string())
  }
}

/// A price (bid and ask) of a unique pair.
#[derive(PartialEq, Clone, Serialize, Debug)]
pub struct Price {
  pair: Pair,
  bid: f64,
  ask: f64,
}

impl Price {
  pub fn new(pair: Pair, bid: f64, ask: f64) -> Self {
    Self {
      pair,
      bid,
      ask,
    }
  }

  pub fn pair(&self) -> &Pair {
    &self.pair
  }

  pub fn bid(&self) -> f64 {
    self.bid
  }

  pub fn ask(&self) -> f64 {
    self.ask
  }
}

impl From<models::Price> for Price {
  fn from(record: models::Price) -> Self {
    Self {
      pair: Pair(record.base, record.quote),
      bid: record.bid,
      ask: record.ask,
    }
  }
}

/// A block of the Concordium blockchain.
#[derive(PartialEq, Clone, Serialize, Debug)]
pub struct Block {
  id: i32,
  height: i64,
  hash: String,
  slot_time_ms: i64,
  baker: i64,
}

impl Block {
  pub fn get_height(&self) -> i64 {
    self.height
  }
}

impl From<models::Block> for Block {
  fn from(record: models::Block) -> Self {
    Self {
      id: record.id,
      height: record.height,
      hash: record.hash,
      slot_time_ms: record.slot_time_ms,
      baker: record.baker,
    }
  }
}

#[derive(Serialize, Debug)]
pub struct Status {
  id: i32,
  resources: models::ResourceStatusJson,
  node: Option<models::NodeStatusJson>,
  timestamp_ms: i64,
}

impl From<models::Status> for Status {
  fn from(record: models::Status) -> Self {
    Self {
      id: record.id,
      resources: record.resources,
      node: record.node,
      timestamp_ms: record.timestamp_ms,
    }
  }
}

#[derive(Serialize, Debug)]
pub struct User {
  id: i32,
  username: String,

  #[serde(skip)]
  password: String,
}

impl User {
  pub fn get_id(&self) -> i32 {
    self.id
  }

  pub fn get_username(&self) -> &str {
    &self.username
  }

  /// It returns if the value corresponds to the hash of the password.
  pub fn check_password(&self, value: &str) -> bool {
    crate::authentication::verify_password(value, &self.password)
  }
}

impl From<models::User> for User {
  fn from(u: models::User) -> Self {
    Self {
      id: u.id,
      username: u.username,
      password: u.password,
    }
  }
}

#[derive(PartialEq, Debug)]
pub struct Session {
  id: String,
  user_id: i32,
  expiration_ms: i64,
  last_use_ms: i64,
}

impl Session {
  pub fn get_user_id(&self) -> i32 {
    self.user_id
  }

  pub fn get_refresh_token(&self) -> &str {
    &self.id
  }
}

impl From<models::Session> for Session {
  fn from(s: models::Session) -> Self {
    Self {
      id: s.id,
      user_id: s.user_id,
      expiration_ms: s.expiration_ms,
      last_use_ms: s.last_use_ms,
    }
  }
}

/// It takes a string of a numeric value and tries to convert it into a decimal
/// instance, otherwise it returns zero.
fn to_decimal(value: &str) -> Decimal {
  Decimal::from_str_exact(value).unwrap_or(Decimal::ZERO)
}
