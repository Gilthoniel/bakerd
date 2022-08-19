use super::{AppError, AsyncJob};
use crate::client::DynNodeClient;
use crate::repository::account::NewAccount;
use crate::repository::DynAccountRepository;
use rust_decimal::Decimal;

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
        // 1. Get the latest block hash of the consensus to get the most up to
        //    date information.
        let last_block = self.client.get_last_block().await?;

        // 2. Get the balance of the account.
        let info = self
            .client
            .get_account_info(&last_block.hash, address)
            .await?;

        // The response contains the total amount of CCD for the account but we
        // store only the available (and the staked) amount.
        let staked = info
            .account_baker
            .map(|b| b.staked_amount)
            .unwrap_or(Decimal::ZERO);

        // 3. Get the lottery power of the account.
        let baker = self.client.get_baker(&last_block.hash, address).await?;

        // 4. Finally the account is updated in the repository.
        let mut new_account = NewAccount {
            address: address.into(),
            available_amount: (info.account_amount - staked).to_string(),
            staked_amount: staked.to_string(),
            lottery_power: 0.0,
        };

        if let Some(baker) = baker {
            new_account.lottery_power = baker.baker_lottery_power;
        }

        self.repository.set_account(new_account).await?;

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
    use crate::client::node::{AccountInfo, Baker, Block, MockNodeClient};
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
            .returning(|| {
                Ok(Block {
                    hash: ":hash:".to_string(),
                    height: 0,
                })
            });

        client
            .expect_get_account_info()
            .with(eq(":hash:"), eq(":address:"))
            .times(1)
            .returning(|_, _| {
                Ok(AccountInfo {
                    account_nonce: 0,
                    account_amount: dec!(42),
                    account_index: 123,
                    account_address: ":address:".into(),
                    account_baker: None,
                })
            });

        client
            .expect_get_baker()
            .with(eq(":hash:"), eq(":address:"))
            .times(1)
            .returning(|_, _| {
                Ok(Some(Baker {
                    baker_account: ":address:".into(),
                    baker_id: 1,
                    baker_lottery_power: 0.5,
                }))
            });

        let mut repository = MockAccountRepository::new();

        repository
            .expect_set_account()
            .withf(|account| {
                account.lottery_power == 0.5
                    && account.available_amount == "42"
                    && account.staked_amount == "0"
            })
            .times(1)
            .returning(|_| Ok(()));

        let mut job = RefreshAccountsJob::new(Arc::new(client), Arc::new(repository));
        job.follow_account(":address:");

        let res = job.execute().await;

        assert!(matches!(res, Ok(_)));
    }
}
