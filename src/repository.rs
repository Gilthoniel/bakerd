use async_trait::async_trait;
use std::sync::Arc;

use crate::model::Account;

#[derive(Debug)]
pub enum RepoError {}

#[async_trait]
pub trait Repository {
  async fn get_account(&self, addr: &str) -> Result<Account, RepoError>;
}

pub type DynRepository = Arc<dyn Repository + Send + Sync>;

pub struct SqliteRepository;

#[async_trait]
impl Repository for SqliteRepository {
  async fn get_account(&self, addr: &str) -> Result<Account, RepoError> {
    Ok(Account::new(""))
  }
}
