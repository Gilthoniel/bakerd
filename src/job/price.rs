use super::{AppError, AsyncJob};
use crate::client::PriceClient;
use crate::model::Pair;

pub struct PriceRefresher<C: PriceClient> {
    client: C,
}

impl<C: PriceClient> PriceRefresher<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }
}

#[async_trait]
impl<C: PriceClient + Sync + Send> AsyncJob for PriceRefresher<C> {
    async fn execute(&self) -> Result<(), AppError> {
        let prices = self
            .client
            .get_prices(vec![Pair::from(("BTC", "USD"))])
            .await?;

        println!("{:?}", prices);

        Ok(())
    }
}
