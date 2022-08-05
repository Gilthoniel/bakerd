use serde::Deserialize;
use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;

#[derive(PartialEq, Eq, Hash, Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Job {
    AccountsRefresher,
}

impl Job {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AccountsRefresher => "accounts-refresher",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    listen_address: SocketAddr,
    jobs: HashMap<Job, String>,
}

impl Config {
    pub fn from_reader(reader: &mut impl io::Read) -> io::Result<Self> {
        serde_yaml::from_reader::<_, Config>(reader)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    pub fn get_listen_addr(&self) -> &SocketAddr {
        &self.listen_address
    }

    pub fn get_jobs(&self) -> &HashMap<Job, String> {
        &self.jobs
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_address: SocketAddr::from(([127, 0, 0, 1], 0)),
            jobs: HashMap::new(),
        }
    }
}
