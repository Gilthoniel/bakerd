pub mod auth;

use crate::authentication::Claims;
use crate::model::{Account, Block, Price, Reward, Status};
use crate::repository::*;
use axum::{
  extract::{Extension, Path, Query},
  http::StatusCode,
  response::{IntoResponse, Response},
  Json,
};
use log::error;
use serde::Deserialize;
use serde_json::json;

type Result<T> = std::result::Result<T, AppError>;

/// An global definition of errors for the application.
#[derive(Debug)]
pub enum AppError {
  AccountNotFound,
  PriceNotFound,
  WrongCredentials,
  Forbidden,
  Internal,
}

impl IntoResponse for AppError {
  /// It transforms the error into a human-readable response. Each error has a
  /// status and a message, or a default internal server error.
  fn into_response(self) -> Response {
    let (status, message) = match self {
      Self::AccountNotFound => (StatusCode::NOT_FOUND, "account does not exist"),
      Self::PriceNotFound => (StatusCode::NOT_FOUND, "price does not exist"),
      Self::WrongCredentials => (StatusCode::UNAUTHORIZED, "wrong credentials"),
      Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
      Self::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal server error"),
    };

    let body = Json(json!({
        "code": status.as_u16(),
        "error": message,
    }));

    (status, body).into_response()
  }
}

/// A controller to return the status of the application.
pub async fn get_status(Extension(repository): Extension<DynStatusRepository>, _: Claims) -> Result<Json<Status>> {
  let status = repository.get_last_report().await.map_err(map_internal_error)?;

  Ok(status.into())
}

/// A controller to return the account associated with the address.
pub async fn get_account(
  Path(addr): Path<String>,
  Extension(repo): Extension<DynAccountRepository>,
  _: Claims,
) -> Result<Json<Account>> {
  let account = repo.get_account(&addr).await.map_err(map_account_error)?;

  Ok(account.into())
}

/// A controller to return the rewards of an account.
pub async fn get_account_rewards(
  Path(addr): Path<String>,
  Extension(repository): Extension<DynAccountRepository>,
  _: Claims,
) -> Result<Json<Vec<Reward>>> {
  let account = repository.get_account(&addr).await.map_err(map_account_error)?;

  let rewards = repository.get_rewards(&account).await.map_err(map_internal_error)?;

  Ok(rewards.into())
}

/// A controller to return the price of a pair. The pair is represented with
/// `${base}:${quote}` as an identifier.
pub async fn get_price(
  Path(pair): Path<String>,
  Extension(repository): Extension<DynPriceRepository>,
  _: Claims,
) -> Result<Json<Price>> {
  // Split the identifier into the two parts. If unsuccessful, an default
  // empty pair is returned.
  let parts = pair.as_str().split_once(':').unwrap_or(("", ""));

  let price = repository.get_price(&parts.into()).await.map_err(|e| match e {
    RepositoryError::NotFound => AppError::PriceNotFound,
    _ => {
      error!("unable to find a price: {}", e);

      AppError::Internal
    }
  })?;

  Ok(price.into())
}

#[derive(Debug, Deserialize)]
pub struct BlockFilter {
  baker: Option<i64>,
  since_ms: Option<i64>,
}

/// A controller to return the list of blocks indexed in the storage. The list can be filtered by
/// baker and slot time.
pub async fn get_blocks(
  params: Query<BlockFilter>,
  repository: Extension<DynBlockRepository>,
  _: Claims,
) -> Result<Json<Vec<Block>>> {
  let filter = models::BlockFilter {
    baker: params.baker,
    since_ms: params.since_ms,
  };

  let blocks = repository.get_all(filter).await.map_err(map_internal_error)?;

  Ok(blocks.into())
}

fn map_internal_error(e: RepositoryError) -> AppError {
  error!("internal server error: {}", e);

  AppError::Internal
}

fn map_account_error(e: RepositoryError) -> AppError {
  match e {
    RepositoryError::NotFound => AppError::AccountNotFound,
    _ => {
      error!("unable to read the account: {}", e);

      AppError::Internal
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::model::{Pair, Status as StatusView};
  use crate::repository::{models, MockAccountRepository, MockPriceRepository, MockStatusRepository};
  use axum::http::StatusCode;
  use diesel::result::Error;
  use mockall::predicate::*;
  use std::sync::Arc;

  #[test]
  fn test_app_errors() {
    assert_eq!(
      StatusCode::NOT_FOUND,
      AppError::AccountNotFound.into_response().status(),
    );

    assert_eq!(StatusCode::NOT_FOUND, AppError::PriceNotFound.into_response().status(),);

    assert_eq!(
      StatusCode::INTERNAL_SERVER_ERROR,
      AppError::Internal.into_response().status(),
    );
  }

  #[tokio::test]
  async fn test_status() {
    let mut repository = MockStatusRepository::new();

    repository.expect_get_last_report().times(1).returning(|| {
      Ok(StatusView::from(models::Status {
        id: 1,
        resources: models::ResourceStatusJson {
          avg_cpu_load: Some(0.5),
          mem_free: Some(256),
          mem_total: Some(512),
          uptime_secs: Some(16),
        },
        node: None,
        timestamp_ms: 0,
      }))
    });

    let res = get_status(Extension(Arc::new(repository)), Claims::default()).await;

    assert!(matches!(res, Ok(_)));
  }

  #[tokio::test]
  async fn test_status_internal_error() {
    let mut repository = MockStatusRepository::new();

    repository
      .expect_get_last_report()
      .times(1)
      .returning(|| Err(RepositoryError::NotFound));

    let res = get_status(Extension(Arc::new(repository)), Claims::default()).await;

    assert!(matches!(res, Err(AppError::Internal)));
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

    let res = get_account(
      Path(":address:".into()),
      Extension(Arc::new(repository)),
      Claims::default(),
    )
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
      .returning(|_| Err(RepositoryError::NotFound));

    let res = get_account(
      Path(addr.to_string()),
      Extension(Arc::new(repository)),
      Claims::default(),
    )
    .await;

    assert!(matches!(res, Err(AppError::AccountNotFound)));
  }

  #[tokio::test]
  async fn test_get_account_internal_error() {
    let mut repository = MockAccountRepository::new();

    let addr = "some-address";

    repository
      .expect_get_account()
      .with(eq(addr))
      .times(1)
      .returning(|_| Err(RepositoryError::Faillable(Box::new(Error::AlreadyInTransaction))));

    let res = get_account(
      Path(addr.to_string()),
      Extension(Arc::new(repository)),
      Claims::default(),
    )
    .await;

    assert!(matches!(res, Err(AppError::Internal)));
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
      Claims::default(),
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

    let res = get_price(
      Path("CCD:USD".into()),
      Extension(Arc::new(repository)),
      Claims::default(),
    )
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
      .returning(move |_| Err(RepositoryError::NotFound));

    let res = get_price(Path("".into()), Extension(Arc::new(repository)), Claims::default()).await;

    assert!(matches!(res, Err(AppError::PriceNotFound)));
  }

  #[tokio::test]
  async fn test_get_blocks() {
    let mut repository = MockBlockRepository::new();

    repository
      .expect_get_all()
      .with(eq(models::BlockFilter {
        baker: Some(42),
        since_ms: Some(1200),
      }))
      .times(1)
      .returning(|_| {
        Ok(vec![Block::from(models::Block {
          id: 1,
          height: 100,
          hash: ":hash-block-100:".into(),
          slot_time_ms: 1500,
          baker: 42,
        })])
      });

    let filter = BlockFilter {
      baker: Some(42),
      since_ms: Some(1200),
    };

    let res = get_blocks(Query(filter), Extension(Arc::new(repository)), Claims::default()).await;

    assert!(matches!(&res, Ok(_)), "wrong result: {:?}", res);
  }
}
