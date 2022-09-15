use super::{AsyncJob, Status};
use crate::client::node::Block;
use crate::client::DynNodeClient;
use crate::model::Account;
use crate::repository::{DynAccountRepository, NewAccount};
use rust_decimal::Decimal;

pub struct RefreshAccountsJob {
  client: DynNodeClient,
  repository: DynAccountRepository,
}

impl RefreshAccountsJob {
  pub fn new(client: DynNodeClient, repository: DynAccountRepository) -> Self {
    Self {
      client,
      repository,
    }
  }

  async fn do_account(&self, last_block: &Block, account: &Account) -> Status {
    // Get the balance of the account.
    let info = self
      .client
      .get_account_info(&last_block.hash, account.get_address())
      .await?;

    // The response contains the total amount of CCD for the account but we
    // store only the available (and the staked) amount.
    let stake = info.account_baker.map(|b| b.staked_amount).unwrap_or(Decimal::ZERO);

    // Get the lottery power of the account.
    let baker = self.client.get_baker(&last_block.hash, account.get_address()).await?;

    // Finally the account is updated in the repository.
    let mut new_account = NewAccount {
      address: account.get_address().into(),
      balance: (info.account_amount - stake).into(),
      stake: stake.into(),
      lottery_power: 0.0,
      pending_update: false,
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
  async fn execute(&self) -> Status {
    let accounts = self.repository.get_for_update().await?;

    // Get the latest block hash of the consensus to get the most up to date information.
    let last_block = self.client.get_last_block().await?;

    for account in accounts {
      self.do_account(&last_block, &account).await?;
    }

    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::client::node::{AccountInfo, Baker, Block, MockNodeClient};
  use crate::repository::MockAccountRepository;
  use mockall::predicate::*;
  use rust_decimal::Decimal;
  use rust_decimal_macros::dec;
  use std::sync::Arc;

  #[tokio::test]
  async fn test_execute() {
    let mut client = MockNodeClient::new();

    client.expect_get_last_block().with().times(1).returning(|| {
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
          account_amount: Decimal::from(42),
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
      .expect_get_for_update()
      .with()
      .times(1)
      .returning(|| Ok(vec![Account::new(1, ":address:", dec!(0), dec!(0), 0.0)]));

    repository
      .expect_set_account()
      .withf(|account| {
        account.lottery_power == 0.5
          && account.balance.0 == Decimal::from(42)
          && account.stake.0 == Decimal::from(0)
          && !account.pending_update
      })
      .times(1)
      .returning(|_| Ok(()));

    let job = RefreshAccountsJob::new(Arc::new(client), Arc::new(repository));

    let res = job.execute().await;

    assert!(matches!(res, Ok(_)));
  }
}
