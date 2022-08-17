use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    Json,
};

use crate::client::Error as ClientError;
use crate::model::{Account, Price};
use crate::repository::{account::DynAccountRepository, price::DynPriceRepository, StorageError};

/// An global definition of errors for the application.
#[derive(Debug)]
pub enum AppError {
    Storage(StorageError),
    Client(ClientError),
}

impl IntoResponse for AppError {
    /// It transforms the error into a human-readable response. Each error has a
    /// status and a message, or a default internal server error.
    fn into_response(self) -> Response {
        match self {
            Self::Storage(e) => e.status_code(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "node error"),
        }
        .into_response()
    }
}

impl From<StorageError> for AppError {
    /// It builds an application error from a storage error.
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}

impl From<ClientError> for AppError {
    /// It builds an application error from a client error.
    fn from(e: ClientError) -> Self {
        Self::Client(e)
    }
}

/// Controller to return the status of the application.
pub async fn status() -> Html<&'static str> {
    Html("{}")
}

/// A controller to return the account associated with the address.
pub async fn get_account(
    Path(addr): Path<String>,
    Extension(repo): Extension<DynAccountRepository>,
) -> Result<Json<Account>, AppError> {
    let account = repo.get_account(&addr).await?;

    Ok(account.into())
}

/// A controller to return the price of a pair. The pair is represented with
/// `${base}:${quote}` as an identifier.
pub async fn get_price(
    Path(pair): Path<String>,
    Extension(repository): Extension<DynPriceRepository>,
) -> Result<Json<Price>, AppError> {
    // Split the identifier into the two parts. If unsuccessful, an default
    // empty pair is returned.
    let parts = pair.as_str().split_once(':').unwrap_or(("", ""));

    let price = repository.get_price(&parts.into()).await?;

    Ok(price.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Pair;
    use crate::repository::account::MockAccountRepository;
    use crate::repository::price::MockPriceRepository;
    use axum::http::StatusCode;
    use diesel::result::Error as DriverError;
    use mockall::predicate::*;
    use std::sync::Arc;
    use tonic::Status;

    #[test]
    fn test_app_errors() {
        assert_eq!(
            StatusCode::INTERNAL_SERVER_ERROR,
            AppError::from(ClientError::Grpc(Status::already_exists("fake")))
                .into_response()
                .status(),
        );

        assert_eq!(
            StatusCode::NOT_FOUND,
            AppError::from(StorageError::Driver(DriverError::NotFound))
                .into_response()
                .status(),
        );
    }

    #[tokio::test]
    async fn test_get_account() {
        let mut repository = MockAccountRepository::default();

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

    #[tokio::test]
    async fn test_get_account_not_found() {
        let mut repository = MockAccountRepository::default();

        let addr = "some-address";

        repository
            .expect_get_account()
            .with(eq(addr))
            .times(1)
            .returning(|_| Err(StorageError::Driver(DriverError::NotFound)));

        let res = get_account(Path(addr.to_string()), Extension(Arc::new(repository))).await;

        assert!(
            matches!(res, Err(AppError::Storage(e)) if e.status_code().0 == StatusCode::NOT_FOUND)
        );
    }

    #[tokio::test]
    async fn test_get_price() {
        let mut repository = MockPriceRepository::new();

        let pair = Pair::from(("CCD", "USD"));
        let bid = 0.1;
        let ask = 0.2;

        repository
            .expect_get_price()
            .with(eq(pair))
            .times(1)
            .returning(move |pair| Ok(Price::new(pair.clone(), bid, ask)));

        let res = get_price(Path("CCD:USD".into()), Extension(Arc::new(repository)))
            .await
            .unwrap();

        assert_eq!(bid, res.bid());
        assert_eq!(ask, res.ask());
    }

    #[tokio::test]
    async fn test_get_price_not_found() {
        let mut repository = MockPriceRepository::new();

        let pair = Pair::from(("", ""));

        repository
            .expect_get_price()
            .with(eq(pair))
            .times(1)
            .returning(move |_| Err(StorageError::Driver(DriverError::NotFound)));

        let res = get_price(Path("".into()), Extension(Arc::new(repository))).await;

        assert!(
            matches!(res, Err(AppError::Storage(e)) if e.status_code().0 == StatusCode::NOT_FOUND)
        );
    }
}
