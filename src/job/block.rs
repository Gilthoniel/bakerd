use super::{AppError, AsyncJob};
use crate::client::DynNodeClient;
use crate::repository::DynBlockRepository;

pub struct BlockFetcher {
    client: DynNodeClient,
    repository: DynBlockRepository,
}

impl BlockFetcher {
    pub fn new(client: DynNodeClient, repository: DynBlockRepository) -> Self {
        Self { client, repository }
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
            println!("doing block");

            height += 1;
        }

        Ok(())
    }
}
