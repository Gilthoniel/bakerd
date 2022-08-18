use log::{info, warn};
use super::{AppError, AsyncJob};
use crate::client::DynNodeClient;
use crate::repository::DynBlockRepository;
use crate::repository::block::NewBlock;

pub struct BlockFetcher {
    client: DynNodeClient,
    repository: DynBlockRepository,
}

impl BlockFetcher {
    pub fn new(client: DynNodeClient, repository: DynBlockRepository) -> Self {
        Self { client, repository }
    }

    async fn do_block(&self, block_hash: &str) -> Result<(), AppError> {
        let info = self.client.get_block_info(block_hash).await?;

        let new_block = NewBlock{
            hash: info.hash,
            height: info.height,
            slot_time_ms: info.slot_time_ms,
            baker: info.baker.unwrap_or(0),
        };

        self.repository.store(new_block).await?;

        info!("block at height `{}` has been processed successfullly", info.height);

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

        while height <= last_block.height {
            match self.client.get_block_at_height(height).await? {
                Some(block_hash) => self.do_block(&block_hash).await?,
                None => {
                    warn!("unable to find a proper hash for height {}", height);
                    return Ok(());
                },
            }

            height += 1;
        }

        Ok(())
    }
}
