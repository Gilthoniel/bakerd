use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize};

use super::Result;
use crate::model::Pair;

const TICKERS_URL: &'static str = "https://api-pub.bitfinex.com/v2/tickers";

pub struct Price {
  pub pair: Pair,
  pub bid: f64,
  pub ask: f64,
  pub daily_change_relative: f64,
  pub high: f64,
  pub low: f64,
}

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait PriceClient {
  async fn get_prices(&self, pairs: Vec<Pair>) -> Result<Vec<Price>>;
}

/// An abstraction of the execution of an HTTP request. It takes the URL and the
/// query parameters.
#[async_trait]
pub trait Executor: Send + Sync {
  async fn get<R>(&self, url: &str, args: &[(&str, &str)]) -> Result<R>
  where
    R: DeserializeOwned;
}

/// A simple HTTP executor that uses the default client to perform requests.
#[derive(Clone)]
pub struct DefaultExecutor {
  client: Client,
}

#[async_trait]
impl Executor for DefaultExecutor {
  async fn get<R>(&self, url: &str, args: &[(&str, &str)]) -> Result<R>
  where
    R: DeserializeOwned,
  {
    let request = self.client.get(url).query(args);

    let res = request.send().await?.json().await?;

    Ok(res)
  }
}

/// A representation of a price and its derived values for a given pair.
#[derive(Deserialize)]
struct Ticker(String, f64, f64, f64, f64, f64, f64, f64, f64, f64, f64);

/// An implementation of the price client trait that is using the Bitfinex API.
#[derive(Clone)]
pub struct BitfinexClient<E: Executor> {
  executor: E,
}

impl Default for BitfinexClient<DefaultExecutor> {
  fn default() -> Self {
    Self::new(DefaultExecutor {
      client: Client::new(),
    })
  }
}

impl<E: Executor> BitfinexClient<E> {
  fn new(executor: E) -> Self {
    Self {
      executor,
    }
  }
}

#[async_trait]
impl<E: Executor> PriceClient for BitfinexClient<E> {
  async fn get_prices(&self, pairs: Vec<Pair>) -> Result<Vec<Price>> {
    // Build the symbols that can be understood by Bitfinex.
    let symbols = pairs
      .iter()
      .map(|pair| format!("t{}{}", pair.get_base(), pair.get_quote()))
      .reduce(|mut acc, symbol| {
        acc.push(',');
        acc.push_str(symbol.as_str());
        acc
      })
      .unwrap_or(String::default());

    let res = self
      .executor
      .get::<Vec<Ticker>>(TICKERS_URL, &[("symbols", &symbols)])
      .await?;

    let prices = pairs
      .into_iter()
      .zip(res.iter())
      .map(|(pair, ticker)| Price {
        pair: pair,
        bid: ticker.1,
        ask: ticker.3,
        daily_change_relative: ticker.6,
        high: ticker.9,
        low: ticker.10,
      })
      .collect();

    Ok(prices)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::client::Error;
  use mockall::mock;
  use mockall::predicate::*;
  use std::time::Duration;

  mock! {
      pub Executor {
          fn get<'a>(&self, url: &str, args: &[(&'a str, &'a str)]) -> Result<&'static str>;
      }
  }

  #[async_trait]
  impl Executor for MockExecutor {
    async fn get<R>(&self, url: &str, args: &[(&str, &str)]) -> Result<R>
    where
      R: DeserializeOwned,
    {
      let res = self.get(url, args)?;
      Ok(serde_json::from_str(&res).unwrap())
    }
  }

  #[tokio::test]
  async fn test_get_prices() {
    let mut mock = MockExecutor::new();

    mock
      .expect_get()
      .withf(|url, args| url == TICKERS_URL && args.len() == 1 && args[0].1 == "tBTCUSD,tCCDUSD")
      .times(1)
      .returning(|_, _| {
        Ok(
          r#"
            [
                ["tBTCUSD", 2.5, 0.0, 1.128, 0, 0, 0, 0, 0, 0, 0],
                ["tCCDUSD", 0.01, 0.0, 0.02, 0, 0, 0, 0, 0, 0, 0]
            ]
            "#,
        )
      });

    let client = BitfinexClient::new(mock);

    let prices = client
      .get_prices(vec![(1, "BTC", "USD").into(), (2, "CCD", "USD").into()])
      .await
      .expect("tickers request failed");

    assert_eq!(2, prices.len());

    assert_eq!(Pair::from((1, "BTC", "USD")), prices[0].pair);
    assert_eq!(2.5, prices[0].bid);
    assert_eq!(1.128, prices[0].ask);

    assert_eq!(Pair::from((2, "CCD", "USD")), prices[1].pair);
    assert_eq!(0.01, prices[1].bid);
    assert_eq!(0.02, prices[1].ask);
  }

  #[tokio::test]
  async fn test_get_prices_with_client() {
    let client = BitfinexClient::default();

    let res = client.get_prices(vec![(1, "BTC", "USD").into()]).await;

    assert!(matches!(res, Ok(_)));
  }

  #[tokio::test]
  async fn test_get_prices_fail() {
    let client = BitfinexClient::new(DefaultExecutor {
      client: Client::builder().connect_timeout(Duration::ZERO).build().unwrap(),
    });

    let res = client.get_prices(vec![Pair::from((1, "BTC", "USD"))]).await;

    assert!(matches!(res, Err(Error::Http(_))));
  }
}
