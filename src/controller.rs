use crate::client::Error as ClientError;
use crate::model::{Account, Price, Reward, Status};
use crate::repository::{
    DynAccountRepository, DynPriceRepository, DynStatusRepository, RepositoryError,
};
use axum::{
    extract::{Extension, Path},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};

/// An global definition of errors for the application.
#[derive(Debug)]
pub enum AppError {
    Repository(RepositoryError),
    Client(ClientError),
}

impl IntoResponse for AppError {
    /// It transforms the error into a human-readable response. Each error has a
    /// status and a message, or a default internal server error.
    fn into_response(self) -> Response {
        match self {
            Self::Repository(e) => e.status_code(),
            _ => (StatusCode::INTERNAL_SERVER_ERROR, "node error"),
        }
        .into_response()
    }
}

impl From<RepositoryError> for AppError {
    /// It builds an application error from a repository error.
    fn from(e: RepositoryError) -> Self {
        Self::Repository(e)
    }
}

impl From<ClientError> for AppError {
    /// It builds an application error from a client error.
    fn from(e: ClientError) -> Self {
        Self::Client(e)
    }
}

/// Controller to return the status of the application.
pub async fn status(
    Extension(repository): Extension<DynStatusRepository>,
) -> Result<Json<Status>, AppError> {
    let status = repository.get_last_report().await?;

    Ok(status.into())
}

/// A controller to return the account associated with the address.
pub async fn get_account(
    Path(addr): Path<String>,
    Extension(repo): Extension<DynAccountRepository>,
) -> Result<Json<Account>, AppError> {
    let account = repo.get_account(&addr).await?;

    Ok(account.into())
}

/// A controller to return the rewards of an account.
pub async fn get_account_rewards(
    Path(addr): Path<String>,
    Extension(repository): Extension<DynAccountRepository>,
) -> Result<Json<Vec<Reward>>, AppError> {
    let account = repository.get_account(&addr).await?;

    let rewards = repository.get_rewards(&account).await?;

    Ok(rewards.into())
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
    use crate::model::{Pair, Status as StatusView};
    use crate::repository::{models, MockAccountRepository, MockPriceRepository, MockStatusRepository};
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
            AppError::from(RepositoryError::Driver(DriverError::NotFound))
                .into_response()
                .status(),
        );
    }

    #[tokio::test]
    async fn test_status() {
        let mut repository = MockStatusRepository::new();

        repository
            .expect_get_last_report()
            .times(1)
            .returning(|| Ok(StatusView::from(models::Status {
                id: 1,
                resources: models::ResourceStatusJson {
                    avg_cpu_load: Some(0.5),
                    mem_free: Some(256),
                    mem_total: Some(512),
                    uptime_secs: Some(16),
                },
                node: None,
                timestamp_ms: 0,
            })));

        let res = status(Extension(Arc::new(repository))).await;

        assert!(matches!(res, Ok(_)));
    }

    #[tokio::test]
    async fn test_get_account() {
        let mut repository = MockAccountRepository::new();

        let account = Account::from(models::Account {
            id: 1,
            address: ":address:".into(),
            available_amount: "125".into(),
            staked_amount: "50".into(),
            lottery_power: 0.06,
        });

        let expect = account.clone();

        repository
            .expect_get_account()
            .with(eq(":address:"))
            .times(1)
            .return_once(move |_| Ok(account));

        let res = get_account(Path(":address:".into()), Extension(Arc::new(repository)))
            .await
            .unwrap();

        assert_eq!(expect, res.0)
    }

    #[tokio::test]
    async fn test_get_account_not_found() {
        let mut repository = MockAccountRepository::new();

        let addr = "some-address";

        repository
            .expect_get_account()
            .with(eq(addr))
            .times(1)
            .returning(|_| Err(RepositoryError::Driver(DriverError::NotFound)));

        let res = get_account(Path(addr.to_string()), Extension(Arc::new(repository))).await;

        assert!(
            matches!(res, Err(AppError::Repository(e)) if e.status_code().0 == StatusCode::NOT_FOUND)
        );
    }

    #[tokio::test]
    async fn test_get_account_rewards() {
        let mut repository = MockAccountRepository::new();

        repository
            .expect_get_account()
            .with(eq(":address:"))
            .times(1)
            .returning(|_| {
                Ok(Account::from(models::Account {
                    id: 1,
                    address: ":address:".into(),
                    available_amount: "125".into(),
                    staked_amount: "50".into(),
                    lottery_power: 0.06,
                }))
            });

        repository
            .expect_get_rewards()
            .withf(|a| a.get_id() == 1)
            .times(1)
            .returning(|_| {
                Ok(vec![Reward::from(models::Reward {
                    id: 1,
                    account_id: 1,
                    block_hash: ":hash:".to_string(),
                    amount: "25.76".to_string(),
                    epoch_ms: 0,
                    kind: models::RewardKind::TransactionFee,
                })])
            });

        let res = get_account_rewards(
            Path(":address:".to_string()),
            Extension(Arc::new(repository)),
        );

        assert!(matches!(res.await, Ok(rewards) if rewards.len() == 1));
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
            .returning(move |_| Err(RepositoryError::Driver(DriverError::NotFound)));

        let res = get_price(Path("".into()), Extension(Arc::new(repository))).await;

        assert!(
            matches!(res, Err(AppError::Repository(e)) if e.status_code().0 == StatusCode::NOT_FOUND)
        );
    }
}
