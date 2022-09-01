use crate::client::{node::Client, DynNodeClient};
use crate::model::Pair;
use jsonwebtoken::{errors, DecodingKey, EncodingKey};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, prelude::*};
use std::net::SocketAddr;
use tonic::{metadata::AsciiMetadataValue, transport::Uri};

const DEFAULT_SECRET: &str = "IUBePnVgKXFPc2QzZTRuSykuQic5IUt8QlY=";

#[derive(PartialEq, Eq, Hash, Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Job {
  AccountsRefresher,
  PriceRefresher,
  BlockFetcher,
  StatusChecker,
}

impl Job {
  pub fn as_str(&self) -> &'static str {
    match self {
      Self::AccountsRefresher => "accounts-refresher",
      Self::PriceRefresher => "price-refresher",
      Self::BlockFetcher => "block-fetcher",
      Self::StatusChecker => "status-checker",
    }
  }
}

#[derive(Clone, Debug, Deserialize)]
pub struct ClientCfg {
  #[serde(with = "serde_with::rust::display_fromstr")]
  uri: Uri,

  #[serde(with = "serde_with::rust::display_fromstr")]
  token: AsciiMetadataValue,
}

impl ClientCfg {
  pub fn as_client(&self) -> DynNodeClient {
    Client::new(&self.uri, &self.token)
  }
}

/// An implementation of the default values for the node client configuration. It follows the
/// default setup of a Concordium node.
impl Default for ClientCfg {
  fn default() -> Self {
    Self {
      uri: Uri::from_static("http://127.0.0.1:10000"),
      token: AsciiMetadataValue::from_static("rpcadmin"),
    }
  }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
  listen_address: SocketAddr,
  client: Option<ClientCfg>,
  jobs: Option<HashMap<Job, String>>,
  pairs: Option<Vec<Pair>>,
}

impl Config {
  pub fn from_file(path: &str) -> io::Result<Self> {
    let mut file = File::open(path)?;

    Self::from_reader(&mut file)
  }

  pub fn from_reader(reader: &mut impl io::Read) -> io::Result<Self> {
    serde_yaml::from_reader::<_, Config>(reader).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
  }

  /// It opens the secret file if specified in the configuration and read the
  /// value. If none is given, a default secret is used.
  pub fn get_decoding_key(&self, secret_file: Option<&str>) -> io::Result<DecodingKey> {
    self.read_secret(secret_file, DecodingKey::from_base64_secret)
  }

  pub fn get_encoding_key(&self, key_file: Option<&str>) -> io::Result<EncodingKey> {
    self.read_secret(key_file, EncodingKey::from_base64_secret)
  }

  pub fn get_listen_addr(&self) -> &SocketAddr {
    &self.listen_address
  }

  pub fn get_jobs(&self) -> Option<&HashMap<Job, String>> {
    self.jobs.as_ref()
  }

  pub fn get_pairs(&self) -> Option<&Vec<Pair>> {
    self.pairs.as_ref()
  }

  pub fn make_client(&self) -> DynNodeClient {
    match &self.client {
      None => ClientCfg::default().as_client(),
      Some(cfg) => cfg.as_client(),
    }
  }

  fn read_secret<T>(&self, path: Option<&str>, from: impl FnOnce(&str) -> errors::Result<T>) -> io::Result<T> {
    let key = match path {
      Some(path) => {
        let mut file = File::open(path)?;

        let mut secret = String::new();
        file.read_to_string(&mut secret)?;

        from(&secret)
      }
      None => from(DEFAULT_SECRET),
    };

    key.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
  }
}

impl Default for Config {
  fn default() -> Self {
    Self {
      listen_address: SocketAddr::from(([127, 0, 0, 1], 0)),
      client: Some(ClientCfg::default()),
      jobs: None,
      pairs: None,
    }
  }
}
