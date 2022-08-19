use super::{AppError, AsyncJob};
use crate::client::node::BlockInfo;
use crate::client::DynNodeClient;
use crate::repository::account::{NewReward, RewardKind};
use crate::repository::block::NewBlock;
use crate::repository::{DynAccountRepository, DynBlockRepository};
use log::{info, warn};
use rust_decimal::Decimal;
use std::collections::HashSet;

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

    pub fn follow_account(&mut self, address: &str) {
        self.accounts.insert(address.into());
    }

    async fn do_block(&self, block_hash: &str) -> Result<(), AppError> {
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

    async fn do_rewards(&self, block_info: &BlockInfo) -> Result<(), AppError> {
        let summary = self
            .client
            .get_block_summary(&block_info.block_hash)
            .await?;

        for event in summary.special_events {
            if event.tag != "PaydayAccountReward" {
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
                let baker_reward = NewReward {
                    account_id: account.get_id(),
                    block_hash: block_info.block_hash.clone(),
                    amount: to_amount(event.baker_reward),
                    epoch_ms: block_info.block_slot_time.timestamp_millis(),
                    kind: RewardKind::Baker,
                };

                self.account_repository.set_reward(baker_reward).await?;

                // 2. Insert the transaction fee reward.
                let tx_fee = NewReward {
                    account_id: account.get_id(),
                    block_hash: block_info.block_hash.clone(),
                    amount: to_amount(event.transaction_fees),
                    epoch_ms: block_info.block_slot_time.timestamp_millis(),
                    kind: RewardKind::TransactionFee,
                };

                self.account_repository.set_reward(tx_fee).await?;
            }
        }

        Ok(())
    }
}

#[async_trait]
impl AsyncJob for BlockFetcher {
    async fn execute(&self) -> Result<(), AppError> {
        // 1. The last block of the consensus is fetched once to learn about
        //    which blocks need to be caught up.
        let last_block = self.client.get_last_block().await?;

        let current_block = self.block_repository.get_last_block().await?;

        let mut height = current_block.get_height() + 1;
        //let mut height = 3697547;

        while height <= last_block.height {
            match self.client.get_block_at_height(height).await? {
                Some(block_hash) => self.do_block(&block_hash).await?,
                None => {
                    warn!("unable to find a proper hash for height {}", height);
                    return Ok(());
                }
            }

            height += 1;
        }

        Ok(())
    }
}

/// It takes an optional amount as decimal and converts it to a string. If the
/// optional value is empty, zero is returned.
fn to_amount(value: Option<Decimal>) -> String {
    value.unwrap_or(Decimal::ZERO).to_string()
}
