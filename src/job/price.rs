use super::{AsyncJob, Status};
use crate::client::PriceClient;
use crate::model::Pair;
use crate::repository::{models, DynPriceRepository};

type Client = Box<dyn PriceClient + Sync + Send>;

pub struct PriceRefresher {
    client: Client,
    repository: DynPriceRepository,
    pairs: Vec<Pair>,
}

impl PriceRefresher {
    pub fn new<C>(client: C, repository: DynPriceRepository) -> Self
    where
        C: PriceClient + Send + Sync + 'static,
    {
        Self {
            client: Box::new(client),
            repository: repository,
            pairs: vec![],
        }
    }

    /// It adds the pair to the list of followed prices.
    pub fn follow_pair(&mut self, pair: Pair) {
        self.pairs.push(pair);
    }
}

#[async_trait]
impl AsyncJob for PriceRefresher {
    async fn execute(&self) -> Status {
        let prices = self.client.get_prices(&self.pairs).await?;

        for price in prices {
            let new_price = models::Price {
                base: price.pair().base().to_string(),
                quote: price.pair().quote().to_string(),
                bid: price.bid(),
                ask: price.ask(),
            };

            self.repository.set_price(new_price).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Pair, Price};
    use crate::repository::MockPriceRepository;
    use mockall::mock;
    use mockall::predicate::*;
    use std::sync::Arc;

    mock! {
        pub Client {
            fn get_prices(&self, pairs: &Vec<Pair>) -> crate::client::Result<Vec<Price>>;
        }
    }

    #[async_trait]
    impl PriceClient for MockClient {
        async fn get_prices(&self, pairs: &Vec<Pair>) -> crate::client::Result<Vec<Price>> {
            self.get_prices(pairs)
        }
    }

    #[tokio::test]
    async fn test_execute() {
        let mut mock_client = MockClient::new();

        let pair: Pair = ("CCD", "USD").into();

        mock_client
            .expect_get_prices()
            .with(eq(vec![pair.clone()]))
            .times(1)
            .returning(|_| Ok(vec![Price::new(("CCD", "USD").into(), 2.0, 0.5)]));

        let mut mock_repository = MockPriceRepository::new();

        mock_repository
            .expect_set_price()
            .with(eq(models::Price {
                base: "CCD".into(),
                quote: "USD".into(),
                bid: 2.0,
                ask: 0.5,
            }))
            .times(1)
            .returning(|_| Ok(()));

        let mut job = PriceRefresher::new(mock_client, Arc::new(mock_repository));
        job.follow_pair(pair.clone());

        let res = job.execute().await;

        assert!(matches!(res, Ok(())));
    }
}
