use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::repository::block::records::Block as BlockRecord;
use crate::repository::price::PriceRecord;

#[derive(Debug, PartialEq, Serialize)]
pub struct Account {
    address: String,
    available_amount: Decimal,
    staked_amount: Decimal,
    lottery_power: f64,
}

impl Account {
    pub fn new(addr: &str) -> Self {
        Self {
            address: addr.to_string(),
            available_amount: Decimal::ZERO,
            staked_amount: Decimal::ZERO,
            lottery_power: 0.0,
        }
    }

    /// It returns the address unique to the account.
    pub fn get_address(&self) -> &str {
        &self.address
    }

    /// It returns the available amount of the account.
    pub fn get_available(&self) -> &Decimal {
        &self.available_amount
    }

    /// It returns the staked amount of the account.
    pub fn get_staked(&self) -> &Decimal {
        &self.staked_amount
    }

    /// It updates the available and staked amount of the account.
    pub fn set_amount(&mut self, available: Decimal, staked_amount: Decimal) {
        self.available_amount = available;
        self.staked_amount = staked_amount;
    }

    /// It returns the lottery power of the account.
    pub fn get_lottery_power(&self) -> f64 {
        self.lottery_power
    }

    /// It updates the lottery power of the account.
    pub fn set_lottery_power(&mut self, power: f64) {
        self.lottery_power = power;
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
