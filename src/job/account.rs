use super::{AppError, AsyncJob};

use crate::client::DynNodeClient;

pub struct RefreshAccountsJob {
    client: DynNodeClient,
}

impl RefreshAccountsJob {
    pub fn new(client: DynNodeClient) -> Self {
        Self { client }
    }
}

#[async_trait]
impl AsyncJob for RefreshAccountsJob {
    async fn execute(&self) -> Result<(), AppError> {
        let hash = self.client.get_last_block().await?;

        let balances = self.client
            .get_balances(&hash, "3wiw27u3JYBbEG7UEjggEu1jQmQymGQH9TknkUAXuYhhGeba7p")
            .await?;

        println!("{:?}", balances);

        Ok(())
    }
}
