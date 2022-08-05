use super::{AppError, AsyncJob};
use crate::client::NodeClient;
use tonic::transport::Uri;

pub struct RefreshAccountsJob {
    client: NodeClient,
}

impl RefreshAccountsJob {
    pub fn new() -> Self {
        Self {
            client: NodeClient::new(Uri::from_static("http://127.0.0.1:10000")),
        }
    }
}

#[async_trait]
impl AsyncJob for RefreshAccountsJob {
    async fn execute(&self) -> Result<(), AppError> {
        let mut client = self.client.clone();

        client.get_account_info("hash", "addr").await?;

        Ok(())
    }
}
