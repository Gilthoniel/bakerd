use axum::{response::{Html, IntoResponse, Response}, http::StatusCode, extract::{Path, Extension}, Json};

use crate::model::Account;
use crate::repository::{RepoError, account::DynAccountRepository};

#[derive(Debug)]
pub enum AppError {
  RepoError(RepoError),
}

impl IntoResponse for AppError {
  fn into_response(self) -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response()
  }
}

impl From<RepoError> for AppError {
  fn from(e: RepoError) -> Self {
    Self::RepoError(e)
  }
}

pub async fn status() -> Html<&'static str> {
  Html("{}")
}

pub async fn get_account(Path(addr): Path<String>, Extension(repo): Extension<DynAccountRepository>) -> Result<Json<Account>, AppError> {
  let account = repo.get_account(&addr).await?;

  Ok(account.into())
}

#[cfg(test)]
mod tests {
  use mockall::*;
  use mockall::predicate::*;
  use async_trait::async_trait;
  use std::sync::Arc;
  use super::*;
  use crate::repository::AccountRepository;

  mock! {
    pub Repository {  
      fn get_account(&self, addr: &str) -> Result<Account, RepoError>;
    }
  }

  #[async_trait]
  impl AccountRepository for MockRepository {
    async fn get_account(&self, addr: &str) -> Result<Account, RepoError> {
      self.get_account(addr)
    }
  }
  
  #[tokio::test]
  async fn test_get_account() {
    let mut repository = MockRepository::default();

    let addr = "some-address";

    repository
      .expect_get_account()
      .with(eq(addr))
      .times(1)
      .returning(|_| Ok(Account::new(addr)));

    let res = get_account(Path(addr.to_string()), Extension(Arc::new(repository))).await.unwrap();
    assert_eq!(Account::new(addr), res.0)
  }

}
