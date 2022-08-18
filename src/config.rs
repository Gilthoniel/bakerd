use serde::Deserialize;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;

use crate::model::Pair;

#[derive(PartialEq, Eq, Hash, Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Job {
    AccountsRefresher,
    PriceRefresher,
    BlockFetcher,
}

impl Job {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AccountsRefresher => "accounts-refresher",
            Self::PriceRefresher => "price-refresher",
            Self::BlockFetcher => "block-fetcher",
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
    pub fn from_reader(reader: &mut impl io::Read) -> io::Result<Self> {
        serde_yaml::from_reader::<_, Config>(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
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
