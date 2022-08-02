use serde::Serialize;

#[derive(Debug, PartialEq, Serialize)]
pub struct Account {
  address: String,
}

impl Account {
  pub fn new(addr: &str) -> Self {
    Self{
      address: addr.to_string(),
    }
  }
}
