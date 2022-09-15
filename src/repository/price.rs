use super::{AsyncPool, PoolError, RepositoryError, Result};
use crate::model::{Pair, Price};
use crate::schema::pairs::dsl::*;
use crate::schema::prices::dsl::*;
use diesel::prelude::*;
use diesel::replace_into;
use diesel::result::Error;
use std::sync::Arc;

pub mod models {
  use crate::schema::pairs;
  use crate::schema::prices;

  #[derive(Queryable)]
  pub struct Pair {
    pub id: i32,
    pub base: String,
    pub quote: String,
  }

  #[derive(Insertable, PartialEq, Debug)]
  #[diesel(table_name = pairs)]
  pub struct NewPair {
    pub base: String,
    pub quote: String,
  }

  /// Record of an account state on the blockchain.
  #[derive(Queryable, Insertable, PartialEq, Debug)]
  pub struct Price {
    pub pair_id: i32,
    pub bid: f64,
    pub ask: f64,
    pub daily_change_relative: f64,
    pub high: f64,
    pub low: f64,
  }
}

/// A repository to set and get prices of pairs.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait PriceRepository {
  /// It returns the pair associated to the identifier if it exists, otherwise it returns a
  /// convenient error.
  async fn get_pair(&self, pair: i32) -> Result<Pair>;

  /// It returns all the pairs present in the storage.
  async fn get_pairs(&self) -> Result<Vec<Pair>>;

  /// It creates a new pair from the parameters and returns the new one with generated values.
  async fn create_pair(&self, new_pair: models::NewPair) -> Result<Pair>;

  /// It takes a pair and return the price if found in the storage.
  async fn get_price(&self, pair: &Pair) -> Result<Price>;

  /// It takes a price and insert or update the price in the storage.
  async fn set_price(&self, price: models::Price) -> Result<()>;
}

pub type DynPriceRepository = Arc<dyn PriceRepository + Sync + Send>;

/// A repository for prices using SQLite as a database engine.
#[derive(Clone)]
pub struct SqlitePriceRepository {
  pool: AsyncPool,
}

impl SqlitePriceRepository {
  pub fn new(pool: AsyncPool) -> Self {
    Self {
      pool,
    }
  }
}

#[async_trait]
impl PriceRepository for SqlitePriceRepository {
  /// It returns the pair associated to the identifier if it exists, otherwise it returns a
  /// convenient error.
  async fn get_pair(&self, pair: i32) -> Result<Pair> {
    let record: models::Pair = self
      .pool
      .exec(move |mut conn| pairs.filter(id.eq(pair)).first(&mut conn))
      .await
      .map_err(map_not_found)?;

    Ok(Pair::from(record))
  }

  /// It returns all the pairs present in the storage.
  async fn get_pairs(&self) -> Result<Vec<Pair>> {
    let records: Vec<models::Pair> = self.pool.exec(|mut conn| pairs.load(&mut conn)).await?;

    Ok(records.into_iter().map(Pair::from).collect())
  }

  /// It creates a new pair from the parameters and returns the new one with generated values.
  async fn create_pair(&self, new_pair: models::NewPair) -> Result<Pair> {
    let pair: models::Pair = self
      .pool
      .exec(|mut conn| {
        diesel::insert_into(pairs).values(&new_pair).execute(&mut conn)?;

        pairs
          .filter(base.eq(new_pair.base))
          .filter(quote.eq(new_pair.quote))
          .first(&mut conn)
      })
      .await?;

    Ok(Pair::from(pair))
  }

  /// It takes a pair and return the price if found in the storage.
  async fn get_price(&self, pair: &Pair) -> Result<Price> {
    let pair = pair.get_id();

    let record: models::Price = self
      .pool
      .exec(move |mut conn| prices.filter(pair_id.eq(pair)).first(&mut conn))
      .await
      .map_err(map_not_found)?;

    Ok(Price::from(record))
  }

  /// It takes a price and insert or update the price in the storage.
  async fn set_price(&self, price: models::Price) -> Result<()> {
    self
      .pool
      .exec(move |mut conn| replace_into(prices).values(&price).execute(&mut conn))
      .await?;

    Ok(())
  }
}

fn map_not_found(e: PoolError) -> RepositoryError {
  match e {
    PoolError::Driver(Error::NotFound) => RepositoryError::NotFound,
    _ => RepositoryError::from(e),
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::repository::RepositoryError;

  #[tokio::test(flavor = "multi_thread")]
  async fn test_create_and_get_pairs() -> Result<()> {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqlitePriceRepository::new(pool);

    let pair = repository
      .create_pair(models::NewPair{
        base: "ETH".into(),
        quote: "USD".into(),
      })
      .await?;

    let res = repository.get_pairs().await?;
    assert_eq!(1, res.len());
    assert_eq!(pair, res[0]);

    let res = repository.get_pair(pair.get_id()).await?;
    assert_eq!(pair, res);

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_set_price() -> Result<()> {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqlitePriceRepository::new(pool);

    let pair = repository
      .create_pair(models::NewPair {
        base: "CCD".into(),
        quote: "USD".into(),
      })
      .await?;

    repository
      .set_price(models::Price {
        pair_id: pair.get_id(),
        bid: 0.00005,
        ask: 0.00006,
        daily_change_relative: 0.001,
        high: 0.0,
        low: 0.0,
      })
      .await
      .unwrap();

    repository
      .set_price(models::Price {
        pair_id: pair.get_id(),
        bid: 0.5,
        ask: 2.4,
        daily_change_relative: 0.001,
        high: 0.0,
        low: 0.0,
      })
      .await
      .unwrap();

    let res = repository.get_price(&pair).await.unwrap();

    assert_eq!(
      res,
      Price::from(models::Price {
        pair_id: pair.get_id(),
        bid: 0.5,
        ask: 2.4,
        daily_change_relative: 0.001,
        high: 0.0,
        low: 0.0,
      })
    );

    Ok(())
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_set_price_failure() {
    let pool = AsyncPool::open(":memory:").unwrap();

    let repository = SqlitePriceRepository::new(pool);

    let res = repository
      .set_price(models::Price {
        pair_id: 1,
        bid: 0.00005,
        ask: 0.00006,
        daily_change_relative: 0.001,
        high: 0.0,
        low: 0.0,
      })
      .await;

    assert!(matches!(res, Err(RepositoryError::Faillable(_))));
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_price_not_found() {
    let pool = AsyncPool::open(":memory:").unwrap();

    pool.run_migrations().await.unwrap();

    let repository = SqlitePriceRepository::new(pool);

    let pair = Pair::from(models::Pair {
      id: 1,
      base: "".into(),
      quote: "".into(),
    });

    let res = repository.get_price(&pair).await;

    assert!(matches!(res, Err(RepositoryError::NotFound)));
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_price_failure() {
    let pool = AsyncPool::open(":memory:").unwrap();

    let repository = SqlitePriceRepository::new(pool);

    let pair = Pair::from(models::Pair {
      id: 1,
      base: "".into(),
      quote: "".into(),
    });

    let res = repository.get_price(&pair).await;

    assert!(matches!(res, Err(RepositoryError::Faillable(_))));
  }
}
