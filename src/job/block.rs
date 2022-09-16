use super::{AsyncJob, Status};
use crate::client::node::BlockInfo;
use crate::client::DynNodeClient;
use crate::repository::*;
use log::{info, warn};
use rust_decimal::Decimal;
use std::collections::HashMap;

const EVENT_TAG_REWARD: &str = "PaydayAccountReward";

const GC_OFFSET: i64 = 500_000;

pub struct BlockFetcher {
  client: DynNodeClient,
  block_repository: DynBlockRepository,
  account_repository: DynAccountRepository,
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
    }
  }

  /// It processes a block by fetching the data about it and analyzes it to
  /// found relevant events.
  async fn do_block(&self, block_hash: &str) -> Status {
    let info = self.client.get_block_info(block_hash).await?;

    // Insert the account rewards before processing the block.
    self.do_rewards(&info).await?;

    let new_block = NewBlock {
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

    let mut rewards = HashMap::new();

    for event in summary.special_events {
      if event.tag != EVENT_TAG_REWARD {
        continue;
      }

      if let Some(addr) = &event.account {
        rewards.insert(
          addr.clone(),
          (
            event.baker_reward.unwrap_or(Decimal::ZERO),
            event.transaction_fees.unwrap_or(Decimal::ZERO),
          ),
        );
      }
    }

    let accounts = self
      .account_repository
      .get_accounts(AccountFilter {
        addresses: Some(rewards.keys().map(String::as_str).collect()),
      })
      .await?;

    for account in accounts {
      if let Some((baker, fees)) = rewards.get(account.get_address()) {
        info!("rewards found for account `{}`", account.get_address());

        // 1. Insert the baker reward.
        let baker_reward = NewReward {
          account_id: account.get_id(),
          block_hash: block_info.block_hash.clone(),
          amount: (*baker).into(),
          epoch_ms: block_info.block_slot_time.timestamp_millis(),
          kind: RewardKind::Baker,
        };

        self.account_repository.set_reward(baker_reward).await?;

        // 2. Insert the transaction fee reward.
        let tx_fee = NewReward {
          account_id: account.get_id(),
          block_hash: block_info.block_hash.clone(),
          amount: (*fees).into(),
          epoch_ms: block_info.block_slot_time.timestamp_millis(),
          kind: RewardKind::TransactionFee,
        };

        self.account_repository.set_reward(tx_fee).await?;
      }
    }

    self
      .account_repository
      .set_for_update(rewards.keys().cloned().collect(), true)
      .await?;

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

#[cfg(test)]
mod tests {
  use super::*;
  use crate::client::node::{BlockInfo, BlockSummary, Event, MockNodeClient};
  use crate::model::{Account, Block};
  use crate::repository::{MockAccountRepository, MockBlockRepository};
  use chrono::Utc;
  use mockall::predicate::*;
  use rust_decimal::Decimal;
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
        special_events: vec![
          Event {
            tag: EVENT_TAG_REWARD.to_string(),
            account: Some(":address-1:".to_string()),
            baker_reward: Some(Decimal::from(25)),
            transaction_fees: Some(Decimal::from(125)),
            finalization_reward: None,
          },
          Event {
            tag: EVENT_TAG_REWARD.to_string(),
            account: Some(":address-2:".to_string()),
            baker_reward: Some(Decimal::from(5)),
            transaction_fees: Some(Decimal::from(12)),
            finalization_reward: None,
          },
        ],
      })
    });

    let mut block_repository = MockBlockRepository::new();

    block_repository
      .expect_get_last_block()
      .times(1)
      .returning(|| Ok(Block::new(1, 100, ":hash-100:", 0, 42)));

    block_repository.expect_store().times(1).returning(|_| Ok(()));

    block_repository
      .expect_garbage_collect()
      .with(eq(102i64 - GC_OFFSET))
      .times(1)
      .returning(|_| Ok(()));

    let mut account_repository = MockAccountRepository::new();

    account_repository
      .expect_get_accounts()
      .withf(|filter| matches!(&filter.addresses, Some(a) if a.len() == 2))
      .times(1)
      .returning(|_| {
        Ok(vec![
          Account::new(1, ":address-1:", dec!(0), dec!(0), 0.0),
          Account::new(2, ":address-3:", dec!(0), dec!(0), 0.0),
        ])
      });

    account_repository.expect_set_reward().times(2).returning(|_| Ok(()));

    account_repository
      .expect_set_for_update()
      .withf(|addrs, pending| *pending && addrs.len() == 2)
      .times(1)
      .returning(|_, _| Ok(()));

    let job = BlockFetcher::new(
      Arc::new(client),
      Arc::new(block_repository),
      Arc::new(account_repository),
    );

    let res = job.execute().await;

    assert!(matches!(res, Ok(_)));
  }
}
