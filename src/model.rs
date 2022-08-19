use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::repository::account::records::Account as AccountRecord;
use crate::repository::block::records::Block as BlockRecord;
use crate::repository::price::PriceRecord;

#[derive(PartialEq, Clone, Debug, Serialize)]
pub struct Account {
    id: i32,
    address: String,
    available_amount: Decimal,
    staked_amount: Decimal,
    lottery_power: f64,
}

impl From<AccountRecord> for Account {
    /// It creates an account from a record of the storage layer.
    fn from(record: AccountRecord) -> Self {
        Self {
            id: record.id,
            address: record.address,
            available_amount: to_decimal(&record.available_amount),
            staked_amount: to_decimal(&record.staked_amount),
            lottery_power: record.lottery_power,
        }
    }
}

#[derive(PartialEq, Clone, Deserialize, Serialize, Debug)]
pub struct Pair(String, String);

impl Pair {
    pub fn base(&self) -> &str {
        &self.0
    }

    pub fn quote(&self) -> &str {
        &self.1
    }
}

impl From<(&str, &str)> for Pair {
    fn from((base, quote): (&str, &str)) -> Self {
        Self(base.to_string(), quote.to_string())
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

#[derive(PartialEq, Clone, Serialize, Debug)]
pub struct Block {
    height: i64,
    hash: String,
    slot_time_ms: i64,
    baker: i64,
}

impl Block {
    pub fn get_height(&self) -> i64 {
        self.height
    }
}

impl From<BlockRecord> for Block {
    fn from(record: BlockRecord) -> Self {
        Self {
            height: record.height,
            hash: record.hash,
            slot_time_ms: record.slot_time_ms,
            baker: record.baker,
        }
    }
}

/// It takes a string of a numeric value and tries to convert it into a decimal
/// instance, otherwise it returns zero.
fn to_decimal(value: &str) -> Decimal {
    Decimal::from_str_exact(value).unwrap_or(Decimal::ZERO)
}
