use super::{AppError, AsyncJob};
use crate::client::PriceClient;
use crate::model::Pair;
use crate::repository::PriceRepository;

type Client = Box<dyn PriceClient + Sync + Send>;

type Repository = Box<dyn PriceRepository + Sync + Send>;

pub struct PriceRefresher {
    client: Client,
    repository: Repository,
    pairs: Vec<Pair>,
}

impl PriceRefresher {
    pub fn new<C, R>(client: C, repository: R) -> Self
    where
        C: PriceClient + Send + Sync + 'static,
        R: PriceRepository + Send + Sync + 'static,
    {
        Self {
            client: Box::new(client),
            repository: Box::new(repository),
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
    async fn execute(&self) -> Result<(), AppError> {
        let prices = self.client.get_prices(&self.pairs).await?;

        for price in prices {
            self.repository.set_price(&price).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;
    use mockall::predicate::*;

    use crate::model::{Pair, Price};
    use crate::repository::StorageError;

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

    mock! {
        pub Repository {
            fn get_price(&self, pair: &Pair) -> Result<Price, StorageError>;

            fn set_price(&self, price: &Price) -> Result<(), StorageError>;
        }
    }

    #[async_trait]
    impl PriceRepository for MockRepository {
        async fn get_price(&self, pair: &Pair) -> Result<Price, StorageError> {
            self.get_price(pair)
        }

        async fn set_price(&self, price: &Price) -> Result<(), StorageError> {
            self.set_price(price)
        }
    }

    #[tokio::test]
    async fn test_execute() {
        let mut mock_client = MockClient::new();

        let pair: Pair = ("CCD", "USD").into();

        mock_client.expect_get_prices()
            .with(eq(vec![pair.clone()]))
            .times(1)
            .returning(|_| Ok(vec![Price::new(("CCD", "USD").into(), 2.0, 0.5)]));

        let mut mock_repository = MockRepository::new();

        mock_repository.expect_set_price()
            .with(eq(Price::new(("CCD", "USD").into(), 2.0, 0.5)))
            .times(1)
            .returning(|_| Ok(()));

        let mut job = PriceRefresher::new(mock_client, mock_repository);
        job.follow_pair(pair.clone());

        let res = job.execute().await;

        assert!(matches!(res, Ok(())));
    }
}
