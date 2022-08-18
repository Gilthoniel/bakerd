use super::{AppError, AsyncJob};
use crate::client::DynNodeClient;
use crate::repository::block::NewBlock;
use crate::repository::DynBlockRepository;
use log::{info, warn};
use std::collections::HashSet;

pub struct BlockFetcher {
    client: DynNodeClient,
    repository: DynBlockRepository,
    accounts: HashSet<String>,
}

impl BlockFetcher {
    pub fn new(client: DynNodeClient, repository: DynBlockRepository) -> Self {
        Self {
            client,
            repository,
            accounts: HashSet::new(),
        }
    }

    pub fn follow_account(&mut self, address: &str) {
        self.accounts.insert(address.into());
    }

    async fn do_block(&self, block_hash: &str) -> Result<(), AppError> {
        // Insert the account rewards before processing the block.
        self.do_rewards(block_hash).await?;

        let info = self.client.get_block_info(block_hash).await?;

        let new_block = NewBlock {
            hash: info.block_hash,
            height: info.block_height,
            slot_time_ms: info.block_slot_time.timestamp_millis(),
            baker: info.block_baker.unwrap_or(0),
        };

        self.repository.store(new_block).await?;

        info!(
            "block at height `{}` has been processed successfullly",
            info.block_height
        );

        Ok(())
    }

    async fn do_rewards(&self, block_hash: &str) -> Result<(), AppError> {
        let summary = self.client.get_block_summary(block_hash).await?;

        for event in summary.special_events {
            if event.tag != "PaydayAccountReward" {
                continue;
            }

            if matches!(&event.account, Some(addr) if self.accounts.contains(addr)) {
                // TODO: insert reward.
                info!("found reward for account `{}`", event.account.unwrap())
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

        let current_block = self.repository.get_last_block().await?;

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
