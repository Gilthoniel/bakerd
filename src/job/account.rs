use super::{AppError, AsyncJob};

use crate::client::DynNodeClient;
use crate::model::Account;
use crate::repository::DynAccountRepository;

pub struct RefreshAccountsJob {
    client: DynNodeClient,
    repository: DynAccountRepository,
    addresses: Vec<String>,
}

impl RefreshAccountsJob {
    pub fn new(client: DynNodeClient, repository: DynAccountRepository) -> Self {
        Self {
            client,
            repository,
            addresses: vec![],
        }
    }

    pub fn follow_account(&mut self, address: &str) {
        self.addresses.push(address.to_string());
    }

    async fn do_account(&self, address: &str) -> Result<(), AppError> {
        let mut account = Account::new(address);

        // 1. Get the latest block hash of the consensus to get the most up to
        //    date information.
        let hash = self.client.get_last_block().await?;

        // 2. Get the balance of the account.
        let balances = self.client.get_balances(&hash, address).await?;

        account.set_amount(balances.0, balances.1);

        // 3. Get the lottery power of the account.
        let baker = self.client.get_baker(&hash, address).await?;

        if let Some(baker) = baker {
            account.set_lottery_power(baker.lottery_power);
        }

        // 4. Finally the account is updated in the repository.
        self.repository.set_account(&account).await?;

        Ok(())
    }
}

#[async_trait]
impl AsyncJob for RefreshAccountsJob {
    async fn execute(&self) -> Result<(), AppError> {
        for address in &self.addresses {
            self.do_account(address).await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::node::MockNodeClient;
    use crate::client::{Baker, Balance};
    use crate::repository::account::MockAccountRepository;
    use mockall::predicate::*;
    use rust_decimal_macros::dec;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_execute() {
        let mut client = MockNodeClient::new();

        client
            .expect_get_last_block()
            .with()
            .times(1)
            .returning(|| Ok(":hash:".to_string()));

        client
            .expect_get_balances()
            .with(eq(":hash:"), eq(":address:"))
            .times(1)
            .returning(|_, _| Ok(Balance(dec!(1), dec!(2.5))));

        client
            .expect_get_baker()
            .with(eq(":hash:"), eq(":address:"))
            .times(1)
            .returning(|_, _| {
                Ok(Some(Baker {
                    id: 1,
                    lottery_power: 0.5,
                }))
            });

        let mut repository = MockAccountRepository::new();

        repository
            .expect_set_account()
            .withf(|account| {
                account.get_lottery_power() == 0.5
                    && *account.get_available() == dec!(1)
                    && *account.get_staked() == dec!(2.5)
            })
            .times(1)
            .returning(|_| Ok(()));

        let mut job = RefreshAccountsJob::new(Arc::new(client), Arc::new(repository));
        job.follow_account(":address:");

        let res = job.execute().await;

        assert!(matches!(res, Ok(_)));
    }
}
