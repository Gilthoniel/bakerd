use super::{AsyncPool, PoolError, RepositoryError, Result};
use crate::model::{Pair, Price};
use crate::schema::prices::dsl::*;
use diesel::prelude::*;
use diesel::replace_into;
use diesel::result::Error;
use std::sync::Arc;

pub mod models {
    use crate::schema::prices;

    /// Record of an account state on the blockchain.
    #[derive(Queryable, Insertable, PartialEq, Debug)]
    pub struct Price {
        pub base: String,
        pub quote: String,
        pub bid: f64,
        pub ask: f64,
    }
}

/// A repository to set and get prices of pairs.
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait PriceRepository {
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
        Self { pool }
    }
}

#[async_trait]
impl PriceRepository for SqlitePriceRepository {
    async fn get_price(&self, pair: &Pair) -> Result<Price> {
        let base_symbol = pair.base().to_string();
        let quote_symbol = pair.quote().to_string();

        let record: models::Price = self
            .pool
            .exec(|mut conn| {
                prices
                    .filter(base.eq(base_symbol))
                    .filter(quote.eq(quote_symbol))
                    .first(&mut conn)
            })
            .await
            .map_err(|e| match e {
                PoolError::Driver(Error::NotFound) => RepositoryError::NotFound,
                _ => RepositoryError::from(e),
            })?;

        Ok(Price::from(record))
    }

    async fn set_price(&self, price: models::Price) -> Result<()> {
        self.pool
            .exec(move |mut conn| replace_into(prices).values(&price).execute(&mut conn))
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repository::RepositoryError;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_set_price() {
        let pool = AsyncPool::open(":memory:").unwrap();

        pool.run_migrations().await.unwrap();

        let repository = SqlitePriceRepository::new(pool);

        repository
            .set_price(models::Price {
                base: "CCD".to_string(),
                quote: "BTC".to_string(),
                bid: 0.00005,
                ask: 0.00006,
            })
            .await
            .unwrap();

        repository
            .set_price(models::Price {
                base: "CCD".to_string(),
                quote: "USD".to_string(),
                bid: 0.5,
                ask: 2.4,
            })
            .await
            .unwrap();

        let pair = Pair::from(("CCD", "USD"));

        let res = repository.get_price(&pair).await.unwrap();

        assert_eq!(&pair, res.pair());
        assert_eq!(0.5, res.bid());
        assert_eq!(2.4, res.ask());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_price_not_found() {
        let pool = AsyncPool::open(":memory:").unwrap();
        
        pool.run_migrations().await.unwrap();

        let repository = SqlitePriceRepository::new(pool);

        let res = repository.get_price(&Pair::from(("CCD", "USD"))).await;

        assert!(matches!(res, Err(RepositoryError::NotFound)));
    }
}
