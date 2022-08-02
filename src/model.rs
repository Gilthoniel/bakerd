use serde::Serialize;

use crate::repository::account::AccountRecord;

#[derive(Debug, PartialEq, Serialize)]
pub struct Account {
  address: String,
}

impl Account {
  pub fn new(addr: &str) -> Self {
    Self {
      address: addr.to_string(),
    }
  }
}

impl From<AccountRecord> for Account {
  fn from(record: AccountRecord) -> Self {
    Account::new(&record.address)
  }
}
