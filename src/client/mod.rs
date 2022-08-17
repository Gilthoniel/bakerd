pub mod bitfinex;
pub mod node;

use crate::model::{Pair, Price};

pub use self::node::Client as NodeClient;

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
