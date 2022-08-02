use axum::{
    extract::{Extension, Path},
    response::{Html, IntoResponse, Response},
    Json,
};

use crate::model::Account;
use crate::repository::{account::DynAccountRepository, StorageError};

#[derive(Debug)]
pub enum AppError {
    Storage(StorageError),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            Self::Storage(e) => e.status_code(),
        }
        .into_response()
    }
}

impl From<StorageError> for AppError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}

/// Controller to return the status of the application.
pub async fn status() -> Html<&'static str> {
    Html("{}")
}

/// Controller to return the account associated with the address.
pub async fn get_account(
    Path(addr): Path<String>,
    Extension(repo): Extension<DynAccountRepository>,
) -> Result<Json<Account>, AppError> {
    let account = repo.get_account(&addr).await?;

    Ok(account.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repository::AccountRepository;
    use async_trait::async_trait;
    use mockall::predicate::*;
    use mockall::*;
    use std::sync::Arc;

    mock! {
      pub Repository {
        fn get_account(&self, addr: &str) -> Result<Account, StorageError>;
      }
    }

    #[async_trait]
    impl AccountRepository for MockRepository {
        async fn get_account(&self, addr: &str) -> Result<Account, StorageError> {
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

        let res = get_account(Path(addr.to_string()), Extension(Arc::new(repository)))
            .await
            .unwrap();
        assert_eq!(Account::new(addr), res.0)
    }
}
