use reqwest::Client;
use serde::{de::DeserializeOwned, Deserialize};

use super::{PriceClient, Result};
use crate::model::{Pair, Price};

const TICKERS_URL: &'static str = "https://api-pub.bitfinex.com/v2/tickers";

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
        Self { executor }
    }
}

#[async_trait]
impl<E: Executor> PriceClient for BitfinexClient<E> {
    async fn get_prices(&self, pairs: &Vec<Pair>) -> Result<Vec<Price>> {
        // Build the symbols that can be understood by Bitfinex.
        let symbols = pairs
            .iter()
            .map(|pair| format!("t{}{}", pair.base(), pair.quote()))
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
            .map(|(pair, ticker)| Price::new(pair.clone(), ticker.1, ticker.3))
            .collect();

        Ok(prices)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockall::mock;
    use mockall::predicate::*;

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

        mock.expect_get()
            .withf(|url, args| {
                url == TICKERS_URL && args.len() == 1 && args[0].1 == "tBTCUSD,tCCDUSD"
            })
            .times(1)
            .returning(|_, _| {
                Ok(r#"
            [
                ["tBTCUSD", 2.5, 0.0, 1.128, 0, 0, 0, 0, 0, 0, 0],
                ["tCCDUSD", 0.01, 0.0, 0.02, 0, 0, 0, 0, 0, 0, 0]
            ]
            "#)
            });

        let client = BitfinexClient::new(mock);

        let prices = client
            .get_prices(&vec![
                Pair::from(("BTC", "USD")),
                Pair::from(("CCD", "USD")),
            ])
            .await
            .expect("tickers request failed");

        assert_eq!(2, prices.len());

        assert_eq!(Pair::from(("BTC", "USD")), *prices[0].pair());
        assert_eq!(2.5, prices[0].bid());
        assert_eq!(1.128, prices[0].ask());

        assert_eq!(Pair::from(("CCD", "USD")), *prices[1].pair());
        assert_eq!(0.01, prices[1].bid());
        assert_eq!(0.02, prices[1].ask());
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::client::Error;
    use std::time::Duration;

    #[tokio::test]
    async fn test_get_prices() {
        let client = BitfinexClient::default();

        let res = client.get_prices(&vec![Pair::from(("BTC", "USD"))]).await;

        assert!(matches!(res, Ok(_)));
    }

    #[tokio::test]
    async fn test_get_prices_fail() {
        let client = BitfinexClient::new(DefaultExecutor {
            client: Client::builder()
                .connect_timeout(Duration::ZERO)
                .build()
                .unwrap(),
        });

        let res = client.get_prices(&vec![Pair::from(("BTC", "USD"))]).await;

        assert!(matches!(res, Err(Error::Http(_))));
    }
}
