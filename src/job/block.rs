use super::{AsyncJob, Status};
use crate::client::node::BlockInfo;
use crate::client::DynNodeClient;
use crate::repository::{models, DynAccountRepository, DynBlockRepository};
use log::{info, warn};
use rust_decimal::Decimal;
use std::collections::HashSet;

const EVENT_TAG_REWARD: &str = "PaydayAccountReward";

const GC_OFFSET: i64 = 500_000;

pub struct BlockFetcher {
  client: DynNodeClient,
  block_repository: DynBlockRepository,
  account_repository: DynAccountRepository,
  accounts: HashSet<String>,
}

impl BlockFetcher {
  pub fn new(
    client: DynNodeClient,
    block_repository: DynBlockRepository,
    account_repository: DynAccountRepository,
  ) -> Self {
    Self {
      client,
      block_repository,
      account_repository,
      accounts: HashSet::new(),
    }
  }

  /// It adds an address of an account to be updated according to the latest
  /// data on the blockchain.
  pub fn follow_account(&mut self, address: &str) {
    self.accounts.insert(address.into());
  }

  /// It processes a block by fetching the data about it and analyzes it to
  /// found relevant events.
  async fn do_block(&self, block_hash: &str) -> Status {
    let info = self.client.get_block_info(block_hash).await?;

    // Insert the account rewards before processing the block.
    self.do_rewards(&info).await?;

    let new_block = models::NewBlock {
      hash: info.block_hash,
      height: info.block_height,
      slot_time_ms: info.block_slot_time.timestamp_millis(),
      baker: info.block_baker.unwrap_or(0),
    };

    self.block_repository.store(new_block).await?;

    info!(
      "block at height `{}` has been processed successfullly",
      info.block_height
    );

    Ok(())
  }

  /// It fetches the special events of a block and tries to find rewards for
  /// the followed accounts.
  async fn do_rewards(&self, block_info: &BlockInfo) -> Status {
    let summary = self.client.get_block_summary(&block_info.block_hash).await?;

    for event in summary.special_events {
      if event.tag != EVENT_TAG_REWARD {
        continue;
      }

      if let Some(addr) = &event.account {
        if !self.accounts.contains(addr) {
          continue;
        }

        info!("found reward for account `{}`", addr);

        // Get the account associated with the reward to get the ID.
        let account = self.account_repository.get_account(addr).await?;

        // 1. Insert the baker reward.
        let baker_reward = models::NewReward {
          account_id: account.get_id(),
          block_hash: block_info.block_hash.clone(),
          amount: to_amount(event.baker_reward),
          epoch_ms: block_info.block_slot_time.timestamp_millis(),
          kind: models::RewardKind::Baker,
        };

        self.account_repository.set_reward(baker_reward).await?;

        // 2. Insert the transaction fee reward.
        let tx_fee = models::NewReward {
          account_id: account.get_id(),
          block_hash: block_info.block_hash.clone(),
          amount: to_amount(event.transaction_fees),
          epoch_ms: block_info.block_slot_time.timestamp_millis(),
          kind: models::RewardKind::TransactionFee,
        };

        self.account_repository.set_reward(tx_fee).await?;
      }
    }

    Ok(())
  }
}

#[async_trait]
impl AsyncJob for BlockFetcher {
  async fn execute(&self) -> Status {
    // The last block of the consensus is fetched once to learn about which
    // blocks need to be caught up.
    let last_block = self.client.get_last_block().await?;

    let current_block = self.block_repository.get_last_block().await?;

    let mut height = current_block.get_height() + 1;

    while height <= last_block.height {
      match self.client.get_block_at_height(height).await? {
        Some(block_hash) => self.do_block(&block_hash).await?,
        None => {
          warn!("unable to find a proper hash for height {}", height);
          break;
        }
      }

      height += 1;
    }

    // Truncate the block table to avoid filling up the space.
    self.block_repository.garbage_collect(height - GC_OFFSET).await?;

    Ok(())
  }
}

/// It takes an optional amount as decimal and converts it to a string. If the
/// optional value is empty, zero is returned.
fn to_amount(value: Option<Decimal>) -> String {
  value.unwrap_or(Decimal::ZERO).to_string()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::client::node::{BlockInfo, BlockSummary, Event, MockNodeClient};
  use crate::model::{Account, Block};
  use crate::repository::{MockAccountRepository, MockBlockRepository};
  use chrono::Utc;
  use mockall::predicate::*;
  use rust_decimal_macros::dec;
  use std::sync::Arc;

  #[tokio::test]
  async fn test_execute() {
    let mut client = MockNodeClient::new();

    client.expect_get_last_block().times(1).returning(|| {
      Ok(crate::client::node::Block {
        hash: ":hash-125:".to_string(),
        height: 125,
      })
    });

    client
      .expect_get_block_at_height()
      .times(2)
      .returning(|height| match height {
        101 => Ok(Some(":hash-101:".to_string())),
        _ => Ok(None),
      });

    client.expect_get_block_info().times(1).returning(|_| {
      Ok(BlockInfo {
        block_hash: ":hash-101:".to_string(),
        block_height: 101,
        finalized: true,
        block_baker: Some(42),
        block_slot_time: Utc::now(),
      })
    });

    client.expect_get_block_summary().times(1).returning(|_| {
      Ok(BlockSummary {
        special_events: vec![Event {
          tag: EVENT_TAG_REWARD.to_string(),
          account: Some(":address:".to_string()),
          baker_reward: Some(dec!(2.5)),
          transaction_fees: Some(dec!(0.125)),
          finalization_reward: None,
        }],
      })
    });

    let mut block_repository = MockBlockRepository::new();

    block_repository.expect_get_last_block().times(1).returning(|| {
      Ok(Block::from(models::Block {
        id: 1,
        height: 100,
        hash: ":hash-100:".to_string(),
        slot_time_ms: 0,
        baker: 42,
      }))
    });

    block_repository.expect_store().times(1).returning(|_| Ok(()));

    block_repository
      .expect_garbage_collect()
      .with(eq(102i64 - GC_OFFSET))
      .times(1)
      .returning(|_| Ok(()));

    let mut account_repository = MockAccountRepository::new();

    account_repository.expect_get_account().times(1).returning(|_| {
      Ok(Account::from(models::Account {
        id: 1,
        address: ":address:".to_string(),
        available_amount: "0".to_string(),
        staked_amount: "0".to_string(),
        lottery_power: 0.0,
      }))
    });

    account_repository.expect_set_reward().times(2).returning(|_| Ok(()));

    let mut job = BlockFetcher::new(
      Arc::new(client),
      Arc::new(block_repository),
      Arc::new(account_repository),
    );

    job.follow_account(":address:");

    let res = job.execute().await;

    assert!(matches!(res, Ok(_)));
  }
}
