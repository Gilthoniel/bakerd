use super::{AsyncJob, Status};
use crate::client::BoxedPriceClient;
use crate::repository::{models, DynPriceRepository};

pub struct PriceRefresher {
  client: BoxedPriceClient,
  repository: DynPriceRepository,
}

impl PriceRefresher {
  pub fn new(client: BoxedPriceClient, repository: DynPriceRepository) -> Self {
    Self {
      client,
      repository,
    }
  }
}

#[async_trait]
impl AsyncJob for PriceRefresher {
  async fn execute(&self) -> Status {
    let pairs = self.repository.get_pairs(models::PairFilter::default()).await?;

    let prices = self.client.get_prices(pairs).await?;

    for price in prices {
      let new_price = models::Price {
        pair_id: price.pair.get_id(),
        bid: price.bid,
        ask: price.ask,
        daily_change_relative: price.daily_change_relative,
        high: price.high,
        low: price.low,
      };

      self.repository.set_price(new_price).await?;
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::client::bitfinex::{MockPriceClient, Price as ClientPrice};
  use crate::repository::{models, MockPriceRepository};
  use mockall::predicate::*;
  use std::sync::Arc;

  #[tokio::test]
  async fn test_execute() {
    let mut mock_client = MockPriceClient::new();

    mock_client
      .expect_get_prices()
      .with(eq(vec![(1, "CCD", "USD").into()]))
      .times(1)
      .returning(|_| {
        Ok(vec![ClientPrice {
          pair: (1, "CCD", "USD").into(),
          bid: 2.0,
          ask: 0.5,
          daily_change_relative: 0.01,
          high: 2.1,
          low: 0.4,
        }])
      });

    let mut mock_repository = MockPriceRepository::new();

    mock_repository
      .expect_get_pairs()
      .withf(|filter| *filter == models::PairFilter::default())
      .times(1)
      .returning(|_| Ok(vec![(1, "CCD", "USD").into()]));

    mock_repository
      .expect_set_price()
      .with(eq(models::Price {
        pair_id: 1,
        bid: 2.0,
        ask: 0.5,
        daily_change_relative: 0.01,
        high: 2.1,
        low: 0.4,
      }))
      .times(1)
      .returning(|_| Ok(()));

    let job = PriceRefresher::new(Box::new(mock_client), Arc::new(mock_repository));

    let res = job.execute().await;

    assert!(matches!(res, Ok(())));
  }
}
