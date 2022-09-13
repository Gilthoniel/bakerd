pub mod bitfinex;
pub mod node;

use self::bitfinex::PriceClient;
use self::node::NodeClient;
use std::fmt;
use std::sync::Arc;

#[derive(Debug)]
pub enum Error {
  Http(reqwest::Error),
  Grpc(tonic::Status),
  Json(serde_json::Error),
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Http(e) => write!(f, "http client error: {}", e),
      Self::Grpc(e) => match std::error::Error::source(e) {
        None => write!(f, "grpc client error: {}", e),
        Some(e) => write!(f, "grpc: {:?}", e),
      },
      Self::Json(e) => write!(f, "client error: encoding: {}", e),
    }
  }
}

impl std::error::Error for Error {}

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

pub type DynNodeClient = Arc<dyn NodeClient + Sync + Send>;

pub type BoxedPriceClient = Box<dyn PriceClient + Sync + Send>;
