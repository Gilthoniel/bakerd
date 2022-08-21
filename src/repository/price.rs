use super::{AsyncPool, Result};
use crate::model::{Pair, Price};
use crate::schema::prices::dsl::*;
use diesel::prelude::*;
use diesel::replace_into;
use std::sync::Arc;

pub mod models {
    /// Record of an account state on the blockchain.
    #[derive(Queryable)]
    pub struct Price {
        pub base: String,
        pub quote: String,
        pub bid: f64,
        pub ask: f64,
    }
}

/// A repository to set and get prices of pairs.
#[async_trait]
pub trait PriceRepository {
    /// It takes a pair and return the price if found in the storage.
    async fn get_price(&self, pair: &Pair) -> Result<Price>;

    /// It takes a price and insert or update the price in the storage.
    async fn set_price(&self, price: &Price) -> Result<()>;
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
        let filter = base.eq(pair.base().to_string());

        let record: models::Price = self
            .pool
            .exec(|mut conn| prices.filter(filter).first(&mut conn))
            .await?;

        Ok(Price::from(record))
    }

    async fn set_price(&self, price: &Price) -> Result<()> {
        let values = (
            base.eq(String::from(price.pair().base())),
            quote.eq(String::from(price.pair().quote())),
            bid.eq(price.bid()),
            ask.eq(price.ask()),
        );

        self.pool
            .exec(|mut conn| replace_into(prices).values(values).execute(&mut conn))
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mockall::mock! {
    pub PriceRepository {
        pub fn get_price(&self, pair: &Pair) -> Result<Price>;

        pub fn set_price(&self, price: &Price) -> Result<()>;
    }
}

#[cfg(test)]
#[async_trait]
impl PriceRepository for MockPriceRepository {
    async fn get_price(&self, pair: &Pair) -> Result<Price> {
        self.get_price(pair)
    }

    async fn set_price(&self, price: &Price) -> Result<()> {
        self.set_price(price)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::repository::RepositoryError;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_set_price() {
        let pool = AsyncPool::new(":memory:");
        pool.run_migrations().await.unwrap();

        let repository = SqlitePriceRepository::new(pool);

        let price = Price::new(Pair::from(("CCD", "USD")), 0.5, 2.4);

        repository.set_price(&price).await.unwrap();

        let pair = Pair::from(("CCD", "USD"));

        let res = repository.get_price(&pair).await.unwrap();

        assert_eq!(&pair, res.pair());
        assert_eq!(0.5, res.bid());
        assert_eq!(2.4, res.ask());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_price_not_found() {
        let pool = AsyncPool::new(":memory:");
        pool.run_migrations().await.unwrap();

        let repository = SqlitePriceRepository::new(pool);

        let res = repository.get_price(&Pair::from(("CCD", "USD"))).await;

        assert!(matches!(res, Err(RepositoryError::Driver(_))));
    }
}
