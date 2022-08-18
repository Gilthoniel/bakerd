pub mod bitfinex;
pub mod node;

use rust_decimal::Decimal;
use std::sync::Arc;

use crate::model::{Pair, Price};

#[derive(Debug)]
pub enum Error {
    Http(reqwest::Error),
    Grpc(tonic::Status),
    Json(serde_json::Error),
}

impl From<reqwest::Error> for Error {
    fn from(e: reqwest::Error) -> Self {
        Self::Http(e)
    }
}

impl From<tonic::Status> for Error {
    fn from(e: tonic::Status) -> Self {
        Self::Grpc(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

#[async_trait]
pub trait PriceClient {
    async fn get_prices(&self, pairs: &Vec<Pair>) -> Result<Vec<Price>>;
}

#[derive(Debug)]
pub struct Balance(pub Decimal, pub Decimal);

#[derive(Debug)]
pub struct Baker {
    pub id: u64,
    pub lottery_power: f64,
}

#[async_trait]
pub trait NodeClient {
    async fn get_last_block(&self) -> Result<String>;

    async fn get_balances(&self, block: &str, address: &str) -> Result<Balance>;

    async fn get_baker(&self, block: &str, address: &str) -> Result<Option<Baker>>;
}

pub type DynNodeClient = Arc<dyn NodeClient + Sync + Send>;
