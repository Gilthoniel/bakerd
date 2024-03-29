use crate::repository::{NodeStatusJson, ResourceStatusJson};
use rust_decimal::Decimal;
use serde::Serialize;

/// An account on the Concordium blockchain. It is uniquely identified through
/// the address.
#[derive(PartialEq, Clone, Debug, Serialize)]
pub struct Account {
  id: i32,
  address: String,
  balance: Decimal,
  stake: Decimal,
  lottery_power: f64,
}

impl Account {
  /// It returns the ID of the account in the storage layer.
  pub fn get_id(&self) -> i32 {
    return self.id;
  }

  pub fn get_address(&self) -> &str {
    return &self.address;
  }
}

impl Account {
  pub fn new(id: i32, address: &str, balance: Decimal, stake: Decimal, lottery_power: f64) -> Self {
    Self {
      id,
      address: address.to_string(),
      balance,
      stake,
      lottery_power,
    }
  }
}

/// A enumeration of the reward kinds. It supports serialization into a human
/// readable string.
#[derive(Serialize, PartialEq, Debug)]
pub enum RewardKind {
  #[serde(rename = "kind_baker")]
  Baker,

  #[serde(rename = "kind_transaction_fee")]
  TransactionFee,
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

impl Reward {
  pub fn new(id: i32, account_id: i32, block_hash: &str, amount: Decimal, epoch_ms: i64, kind: RewardKind) -> Self {
    Self {
      id,
      account_id,
      block_hash: block_hash.to_string(),
      amount,
      epoch_ms,
      kind,
    }
  }

  pub fn get_block_hash(&self) -> &str {
    &self.block_hash
  }

  pub fn get_epoch_ms(&self) -> i64 {
    self.epoch_ms
  }

  pub fn get_kind(&self) -> &RewardKind {
    &self.kind
  }
}

/// A unique combination of a base and a quote currency.
#[derive(Serialize, Debug)]
pub struct Pair {
  id: i32,
  base: String,
  quote: String,
}

impl Pair {
  pub fn get_id(&self) -> i32 {
    self.id
  }

  pub fn get_base(&self) -> &str {
    &self.base
  }

  pub fn get_quote(&self) -> &str {
    &self.quote
  }
}

/// A pair is equal to another when its identifier is the same.
impl PartialEq for Pair {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id
  }

  fn ne(&self, other: &Self) -> bool {
    !self.eq(other)
  }
}

impl From<(i32, &str, &str)> for Pair {
  fn from(v: (i32, &str, &str)) -> Self {
    Self {
      id: v.0,
      base: v.1.into(),
      quote: v.2.into(),
    }
  }
}

/// A price (bid and ask) of a unique pair.
#[derive(PartialEq, Serialize, Debug)]
pub struct Price {
  pair_id: i32,
  bid: f64,
  ask: f64,
  daily_change_relative: f64,
  high: f64,
  low: f64,
}

impl Price {
  pub fn new(pair_id: i32, bid: f64, ask: f64, daily_change_relative: f64, high: f64, low: f64) -> Self {
    Self {
      pair_id,
      bid,
      ask,
      daily_change_relative,
      high,
      low,
    }
  }
}

impl From<(i32, f64, f64)> for Price {
  fn from(v: (i32, f64, f64)) -> Self {
    Self {
      pair_id: v.0,
      bid: v.1,
      ask: v.2,
      daily_change_relative: 0.0,
      high: v.2,
      low: v.1,
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
  pub fn new(id: i32, height: i64, hash: &str, slot_time_ms: i64, baker: i64) -> Self {
    Self {
      id,
      height,
      hash: hash.to_string(),
      slot_time_ms,
      baker,
    }
  }

  pub fn get_height(&self) -> i64 {
    self.height
  }
}

#[derive(Serialize, Debug)]
pub struct Status {
  id: i32,
  resources: ResourceStatusJson,
  node: Option<NodeStatusJson>,
  timestamp_ms: i64,
}

impl Status {
  pub fn new(id: i32, resources: ResourceStatusJson, node: Option<NodeStatusJson>, timestamp_ms: i64) -> Self {
    Self {
      id,
      resources,
      node,
      timestamp_ms,
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
  pub fn new(id: i32, username: &str, password: &str) -> Self {
    Self {
      id,
      username: username.to_string(),
      password: password.to_string(),
    }
  }

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

#[derive(PartialEq, Debug)]
pub struct Session {
  id: String,
  user_id: i32,
  expiration_ms: i64,
  last_use_ms: i64,
}

impl Session {
  pub fn new(id: &str, user_id: i32, expiration_ms: i64, last_use_ms: i64) -> Self {
    Self {
      id: id.to_string(),
      user_id,
      expiration_ms,
      last_use_ms,
    }
  }

  pub fn get_user_id(&self) -> i32 {
    self.user_id
  }

  pub fn get_refresh_token(&self) -> &str {
    &self.id
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_account_attributes() {
    let account = Account {
      id: 1,
      address: ":address:".into(),
      balance: Decimal::from(123),
      lottery_power: 0.0,
      stake: Decimal::from(456),
    };

    // Serialize
    let res = serde_json::to_string(&account);
    assert!(matches!(res, Ok(_)));

    // Debug
    format!("{:?}", account);

    // Clone + PartialEq
    assert!(account == account.clone());
  }

  #[test]
  fn test_reward() {
    let reward = Reward {
      id: 1,
      account_id: 2,
      amount: Decimal::from(123),
      block_hash: ":hash:".into(),
      epoch_ms: 1000,
      kind: RewardKind::Baker,
    };

    assert_eq!(reward.get_block_hash(), ":hash:");
    assert_eq!(1000, reward.get_epoch_ms());
    assert_eq!(*reward.get_kind(), RewardKind::Baker);
  }

  #[test]
  fn test_reward_attributes() {
    let reward = Reward {
      id: 1,
      account_id: 2,
      amount: Decimal::from(123),
      block_hash: ":hash:".into(),
      epoch_ms: 1000,
      kind: RewardKind::Baker,
    };

    // Serialize
    let res = serde_json::to_string(&reward);
    assert!(matches!(res, Ok(_)));

    // Debug
    format!("{:?}", reward);
  }

  #[test]
  fn test_pair_attributes() {
    let pair = Pair {
      id: 1,
      base: "CCD".to_string(),
      quote: "USD".to_string(),
    };

    // Serialize
    let res = serde_json::to_string(&pair);
    assert!(matches!(res, Ok(_)));

    // Debug
    format!("{:?}", pair);
  }

  #[test]
  fn test_price_attributes() {
    let price = Price {
      pair_id: 1,
      bid: 0.5,
      ask: 0.2,
      daily_change_relative: 0.01,
      high: 1.0,
      low: 0.0,
    };

    // Serialize
    let res = serde_json::to_string(&price);
    assert!(matches!(res, Ok(_)));

    // Debug
    format!("{:?}", price);
  }

  #[test]
  fn test_block_attributes() {
    let block = Block {
      id: 1,
      baker: 42,
      hash: ":hash:".into(),
      height: 123,
      slot_time_ms: 1000,
    };

    // Serialize
    let res = serde_json::to_string(&block);
    assert!(matches!(res, Ok(_)));

    // Debug
    format!("{:?}", block);

    // Clone + PartialEq
    assert!(block == block.clone());
  }

  #[test]
  fn test_status_attributes() {
    let status = Status {
      id: 1,
      resources: ResourceStatusJson {
        avg_cpu_load: Some(0.5),
        mem_free: Some(50),
        mem_total: Some(100),
        uptime_secs: Some(1000),
      },
      node: None,
      timestamp_ms: 1000,
    };

    // Serialize
    let res = serde_json::to_string(&status);
    assert!(matches!(res, Ok(_)));

    // Debug
    format!("{:?}", res);
  }

  #[test]
  fn test_user_attributes() {
    let user = User {
      id: 1,
      password: "password".into(),
      username: "username".into(),
    };

    // Serialize
    let res = serde_json::to_string(&user);
    assert!(matches!(res, Ok(_)));

    // Debug
    format!("{:?}", user);
  }
}
