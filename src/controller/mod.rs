pub mod auth;

use crate::authentication::{Claims, Role};
use crate::model::{Account, Block, Pair, Price, Reward, Status};
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
  PairNotFound,
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
      Self::PairNotFound => (StatusCode::NOT_FOUND, "pair does not exist"),
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

#[derive(Deserialize, Debug)]
pub struct CreateAccount {
  address: String,
}

/// A controller to create an account that will be followed by the daemon.
pub async fn create_account(
  request: Json<CreateAccount>,
  repository: Extension<DynAccountRepository>,
  claims: Claims,
) -> Result<Json<Account>> {
  if !claims.has_role(Role::Admin) {
    return Err(AppError::Forbidden);
  }

  let new_account = models::NewAccount::new(&request.address, true);

  repository.set_account(new_account).await.map_err(map_internal_error)?;

  let res = repository
    .get_account(&request.address)
    .await
    .map_err(map_internal_error)?;

  Ok(res.into())
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

#[derive(Deserialize, Debug)]
pub struct CreatePair {
  base: String,
  quote: String,
}

// A controller to create a pair.
pub async fn create_pair(
  request: Json<CreatePair>,
  repository: Extension<DynPriceRepository>,
  claims: Claims,
) -> Result<Json<Pair>> {
  if !claims.has_role(Role::Admin) {
    return Err(AppError::Forbidden);
  }

  let new_pair = models::NewPair {
    base: request.base.clone(),
    quote: request.quote.clone(),
  };

  let pair = repository.create_pair(new_pair).await.map_err(map_internal_error)?;

  Ok(pair.into())
}

#[derive(Deserialize, Debug)]
pub struct PairFilter {
  base: Option<String>,
  quote: Option<String>,
}

/// A controller to return all the pairs.
pub async fn get_pairs(
  query: Query<PairFilter>,
  repository: Extension<DynPriceRepository>,
  _: Claims,
) -> Result<Json<Vec<Pair>>> {
  let filter = models::PairFilter {
    base: query.base.as_ref().map(String::as_str),
    quote: query.quote.as_ref().map(String::as_str),
  };

  let pairs = repository.get_pairs(filter).await.map_err(map_pair_error)?;

  Ok(pairs.into())
}

/// A controller to return the price of a pair.
pub async fn get_price(
  Path(pair_id): Path<i32>,
  Extension(repository): Extension<DynPriceRepository>,
  _: Claims,
) -> Result<Json<Price>> {
  let pair = repository.get_pair(pair_id).await.map_err(map_pair_error)?;

  let price = repository.get_price(&pair).await.map_err(|e| match e {
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

fn map_pair_error(e: RepositoryError) -> AppError {
  match e {
    RepositoryError::NotFound => AppError::PairNotFound,
    _ => {
      error!("unable to find a pair: {}", e);

      AppError::Internal
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::model::{Pair, Status as StatusView};
  use crate::repository::models::dec;
  use crate::repository::{models, MockAccountRepository, MockPriceRepository, MockStatusRepository};
  use axum::http::StatusCode;
  use diesel::result::Error;
  use mockall::predicate::*;
  use std::sync::Arc;

  #[test]
  fn test_app_errors() {
    let tests = vec![
      (StatusCode::NOT_FOUND, AppError::AccountNotFound),
      (StatusCode::NOT_FOUND, AppError::PriceNotFound),
      (StatusCode::NOT_FOUND, AppError::PairNotFound),
      (StatusCode::UNAUTHORIZED, AppError::WrongCredentials),
      (StatusCode::FORBIDDEN, AppError::Forbidden),
      (StatusCode::INTERNAL_SERVER_ERROR, AppError::Internal),
    ];

    for test in tests {
      assert_ne!("", format!("{:?}", &test.1));
      assert_eq!(test.0, test.1.into_response().status());
    }
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

  #[test]
  fn test_create_account_request() {
    let value = "{\"address\":\"some-address\"}";

    let res: CreateAccount = serde_json::from_str(value).expect("it should be deserialized");

    let expect = CreateAccount {
      address: "some-address".into(),
    };

    assert_eq!(format!("{:?}", res), format!("{:?}", expect));
    assert_eq!("some-address", res.address);
  }

  #[tokio::test]
  async fn test_create_account() {
    let mut repository = MockAccountRepository::new();

    repository.expect_set_account().times(1).returning(|_| Ok(()));

    repository.expect_get_account().times(1).returning(|_| {
      Ok(Account::from(models::Account {
        id: 1,
        address: ":address:".into(),
        balance: dec!(42),
        stake: dec!(1),
        lottery_power: 0.6,
        pending_update: false,
      }))
    });

    let request = Json(CreateAccount {
      address: ":address:".to_string(),
    });

    let claims = Claims::builder().roles(vec![Role::Admin]).build();

    let res = create_account(request, Extension(Arc::new(repository)), claims).await;

    assert!(matches!(res, Ok(_)));
  }

  #[tokio::test]
  async fn test_create_account_forbidden() {
    let repository = MockAccountRepository::new();

    let request = Json(CreateAccount {
      address: ":address:".to_string(),
    });

    let claims = Claims::default();

    let res = create_account(request, Extension(Arc::new(repository)), claims).await;

    assert!(matches!(res, Err(AppError::Forbidden)));
  }

  #[tokio::test]
  async fn test_get_account() {
    let mut repository = MockAccountRepository::new();

    let account = Account::from(models::Account {
      id: 1,
      address: ":address:".into(),
      balance: dec!(125),
      stake: dec!(25),
      lottery_power: 0.06,
      pending_update: false,
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
          balance: dec!(125),
          stake: dec!(50),
          lottery_power: 0.06,
          pending_update: false,
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
          amount: dec!(2576),
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

  #[test]
  fn test_create_pair_request() {
    let value = "{\"base\":\"ETH\",\"quote\":\"CHF\"}";

    let res: CreatePair = serde_json::from_str(value).unwrap();

    let expect = CreatePair {
      base: "ETH".into(),
      quote: "CHF".into(),
    };

    assert_eq!(format!("{:?}", res), format!("{:?}", expect));
    assert_eq!(res.base, expect.base);
    assert_eq!(res.quote, expect.quote);
  }

  #[tokio::test]
  async fn test_create_pair() {
    let mut repository = MockPriceRepository::new();

    repository
      .expect_create_pair()
      .with(eq(models::NewPair {
        base: "CCD".into(),
        quote: "USD".into(),
      }))
      .times(1)
      .returning(|new_pair| Ok(Pair::from((1, new_pair.base.as_str(), new_pair.quote.as_str()))));

    let request = Json(CreatePair {
      base: "CCD".into(),
      quote: "USD".into(),
    });

    let claims = Claims::builder().roles(vec![Role::Admin]).build();

    let res = create_pair(request, Extension(Arc::new(repository)), claims).await;

    assert!(matches!(res, Ok(p) if p.0 == Pair::from((1, "CCD", "USD"))));
  }

  #[tokio::test]
  async fn test_get_pairs() {
    let mut repository = MockPriceRepository::new();

    repository.expect_get_pairs().times(1).returning(|_| Ok(vec![]));

    let claims = Claims::default();

    let filter = PairFilter {
      base: None,
      quote: None,
    };

    let res = get_pairs(Query(filter), Extension(Arc::new(repository)), claims).await;

    assert!(matches!(res, Ok(_)));
  }

  #[tokio::test]
  async fn test_get_price() {
    let mut repository = MockPriceRepository::new();

    repository
      .expect_get_pair()
      .with(eq(1))
      .times(1)
      .returning(|id| Ok((id, "CDD", "USD").into()));

    repository
      .expect_get_price()
      .with(eq(Pair::from((1, "CDD", "USD"))))
      .times(1)
      .returning(|_| Ok((1, 0.1, 0.2).into()));

    let res = get_price(Path(1), Extension(Arc::new(repository)), Claims::default())
      .await
      .unwrap();

    assert_eq!(res.0, (1, 0.1, 0.2).into());
  }

  #[tokio::test]
  async fn test_get_price_no_pair() {
    let mut repository = MockPriceRepository::new();

    repository
      .expect_get_pair()
      .with(eq(42))
      .times(1)
      .returning(|_| Err(RepositoryError::NotFound));

    let res = get_price(Path(42), Extension(Arc::new(repository)), Claims::default()).await;

    assert!(matches!(res, Err(AppError::PairNotFound)));
  }

  #[tokio::test]
  async fn test_get_price_pair_failed() {
    let mut repository = MockPriceRepository::new();

    repository
      .expect_get_pair()
      .with(eq(42))
      .times(1)
      .returning(|_| Err(RepositoryError::Faillable(Box::new(Error::AlreadyInTransaction))));

    let res = get_price(Path(42), Extension(Arc::new(repository)), Claims::default()).await;

    assert!(matches!(res, Err(AppError::Internal)));
  }

  #[tokio::test]
  async fn test_get_price_not_found() {
    let mut repository = MockPriceRepository::new();

    repository
      .expect_get_pair()
      .with(eq(42))
      .times(1)
      .returning(|id| Ok((id, "CDD", "USD").into()));

    repository
      .expect_get_price()
      .with(eq(Pair::from((42, "CDD", "USD"))))
      .times(1)
      .returning(|_| Err(RepositoryError::NotFound));

    let res = get_price(Path(42), Extension(Arc::new(repository)), Claims::default()).await;

    assert!(matches!(res, Err(AppError::PriceNotFound)));
  }

  #[tokio::test]
  async fn test_get_price_failed() {
    let mut repository = MockPriceRepository::new();

    repository
      .expect_get_pair()
      .with(eq(42))
      .times(1)
      .returning(|id| Ok((id, "CDD", "USD").into()));

    repository
      .expect_get_price()
      .with(eq(Pair::from((42, "CDD", "USD"))))
      .times(1)
      .returning(|_| Err(RepositoryError::Faillable(Box::new(Error::AlreadyInTransaction))));

    let res = get_price(Path(42), Extension(Arc::new(repository)), Claims::default()).await;

    assert!(matches!(res, Err(AppError::Internal)));
  }

  #[test]
  fn test_block_filter() {
    let value = "{\"baker\":42,\"since_ms\":1000}";

    let res: BlockFilter = serde_json::from_str(value).unwrap();

    let expect = BlockFilter {
      baker: Some(42),
      since_ms: Some(1000),
    };

    assert_eq!(format!("{:?}", res), format!("{:?}", expect));
    assert_eq!(res.baker, expect.baker);
    assert_eq!(res.since_ms, expect.since_ms);
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
