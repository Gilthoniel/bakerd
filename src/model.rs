use serde::{Deserialize, Serialize};

use crate::repository::price::PriceRecord;

#[derive(Debug, PartialEq, Serialize)]
pub struct Account {
    address: String,
}

impl Account {
    pub fn new(addr: &str) -> Self {
        Self {
            address: addr.to_string(),
        }
    }

    pub fn get_address(&self) -> &str {
        &self.address
    }
}

#[derive(PartialEq, Clone, Deserialize, Serialize, Debug)]
pub struct Pair(String, String);

impl From<(&str, &str)> for Pair {
    fn from((base, quote): (&str, &str)) -> Self {
        Self(base.to_string(), quote.to_string())
    }
}

impl Pair {
    pub fn base(&self) -> &str {
        &self.0
    }

    pub fn quote(&self) -> &str {
        &self.1
    }
}

#[derive(PartialEq, Clone, Serialize, Debug)]
pub struct Price {
    pair: Pair,
    bid: f64,
    ask: f64,
}

impl Price {
    pub fn new(pair: Pair, bid: f64, ask: f64) -> Self {
        Self { pair, bid, ask }
    }

    pub fn pair(&self) -> &Pair {
        &self.pair
    }

    pub fn bid(&self) -> f64 {
        self.bid
    }

    pub fn ask(&self) -> f64 {
        self.ask
    }
}

impl From<PriceRecord> for Price {
    fn from(record: PriceRecord) -> Self {
        Self {
            pair: Pair(record.base, record.quote),
            bid: record.bid,
            ask: record.ask,
        }
    }
}
