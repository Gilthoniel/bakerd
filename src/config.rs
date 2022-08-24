use crate::model::Pair;
use jsonwebtoken::DecodingKey;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, prelude::*};
use std::net::SocketAddr;

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

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    listen_address: SocketAddr,
    jobs: Option<HashMap<Job, String>>,
    pairs: Option<Vec<Pair>>,
    accounts: Option<Vec<String>>,
}

impl Config {
    pub fn from_file(path: &str) -> io::Result<Self> {
        let mut file = File::open(path)?;

        Self::from_reader(&mut file)
    }

    pub fn from_reader(reader: &mut impl io::Read) -> io::Result<Self> {
        serde_yaml::from_reader::<_, Config>(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    /// It opens the secret file if specified in the configuration and read the
    /// value. If none is given, a default secret is used.
    pub fn get_secret(&self, secret_file: Option<&str>) -> io::Result<DecodingKey> {
        let key = match secret_file {
            Some(path) => {
                let mut file = File::open(path)?;

                let mut secret = String::new();
                file.read_to_string(&mut secret)?;

                DecodingKey::from_base64_secret(&secret)
            }
            None => DecodingKey::from_base64_secret(DEFAULT_SECRET),
        };

        key.map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
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

    pub fn get_accounts(&self) -> Option<&Vec<String>> {
        self.accounts.as_ref()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_address: SocketAddr::from(([127, 0, 0, 1], 0)),
            jobs: None,
            pairs: None,
            accounts: None,
        }
    }
}
