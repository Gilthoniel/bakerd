use super::{AsyncJob, AppError};

pub struct RefreshAccountsJob;

impl RefreshAccountsJob {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl AsyncJob for RefreshAccountsJob {
    async fn execute(&self) -> Result<(), AppError> {
        Ok(())
    }
}
