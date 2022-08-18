use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use std::sync::Arc;
use tonic::codegen::InterceptedService;
use tonic::metadata::Ascii;
use tonic::service::Interceptor;
use tonic::transport::Uri;
use tonic::{metadata::MetadataValue, transport::Channel, Request, Status};

use ccd::p2p_client::P2pClient;

use super::{Baker, Balance, DynNodeClient, NodeClient, Result};

pub mod ccd {
    tonic::include_proto!("concordium");
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ConsensusInfo {
    last_finalized_block: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct AccountBaker {
    staked_amount: Decimal,
    restake_earnings: bool,
    baker_id: u32,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
struct AccountInfo {
    account_nonce: u32,
    account_amount: Decimal,
    account_index: u32,
    account_address: String,
    account_baker: AccountBaker,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct BirkBaker {
    baker_account: String,
    baker_id: u64,
    baker_lottery_power: f64,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct BirkParameters {
    bakers: Vec<BirkBaker>,
}

pub struct Client {
    client: P2pClient<InterceptedService<Channel, Authorization>>,
}

impl Client {
    pub fn new(addr: &str) -> DynNodeClient {
        // TODO: remove unwrap.
        let uri = Uri::from_str(addr).unwrap();
        let channel = Channel::builder(uri).connect_lazy();

        let client = P2pClient::with_interceptor(channel, Authorization::new());

        Arc::new(Client { client })
    }
}

#[async_trait]
impl NodeClient for Client {
    /// It fetches the current consensus status of the Concordium network of the
    /// node and returns the hash of the last finalized block.
    async fn get_last_block(&self) -> Result<String> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::Empty {});

        let response = client.get_consensus_status(request).await?.into_inner();

        let info: ConsensusInfo = serde_json::from_str(response.value.as_str())?;

        Ok(info.last_finalized_block)
    }

    /// It returns the details of the account like its balance and the staked
    /// amount.
    async fn get_balances(&self, block_hash: &str, address: &str) -> Result<Balance> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::GetAddressInfoRequest {
            block_hash: String::from(block_hash),
            address: String::from(address),
        });

        let response = client.get_account_info(request).await?.into_inner();

        let info: AccountInfo = serde_json::from_str(response.value.as_str())?;

        let available = info.account_amount - info.account_baker.staked_amount;

        Ok(Balance(available, info.account_baker.staked_amount))
    }

    /// It returns the baker of the account address if it exists in the
    /// consensus.
    async fn get_baker(&self, block: &str, address: &str) -> Result<Option<Baker>> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::BlockHash {
            block_hash: block.to_string(),
        });

        let response = client.get_birk_parameters(request).await?.into_inner();

        let params: BirkParameters = serde_json::from_str(response.value.as_str())?;

        for baker in params.bakers {
            if baker.baker_account == address {
                return Ok(Some(Baker {
                    id: baker.baker_id,
                    lottery_power: baker.baker_lottery_power,
                }));
            }
        }

        Ok(None)
    }
}

#[derive(Clone)]
struct Authorization {
    token: MetadataValue<Ascii>,
}

impl Authorization {
    fn new() -> Self {
        let token = MetadataValue::try_from("rpcadmin").expect("authorization token is malformed");

        Authorization { token }
    }
}

impl Interceptor for Authorization {
    fn call(&mut self, mut req: Request<()>) -> std::result::Result<Request<()>, Status> {
        req.metadata_mut()
            .insert("authentication", self.token.clone());

        Ok(req)
    }
}

#[cfg(test)]
mockall::mock! {
    pub NodeClient {
        pub fn get_last_block(&self) -> Result<String>;
        pub fn get_balances(&self, block: &str, address: &str) -> Result<Balance>;
        pub fn get_baker(&self, block: &str, address: &str) -> Result<Option<Baker>>;
    }
}

#[cfg(test)]
#[async_trait]
impl NodeClient for MockNodeClient {
    async fn get_last_block(&self) -> Result<String> {
        self.get_last_block()
    }

    async fn get_balances(&self, block: &str, address: &str) -> Result<Balance> {
        self.get_balances(block, address)
    }

    async fn get_baker(&self, block: &str, address: &str) -> Result<Option<Baker>> {
        self.get_baker(block, address)
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::client::Error;
    use mockall::predicate::*;
    use tonic::{Request, Response, Status};

    type JsonResponse = std::result::Result<Response<ccd::JsonResponse>, Status>;

    mockall::mock! {
        pub Service {
            fn get_consensus_status(&self, request: Request<ccd::Empty>) -> JsonResponse;

            fn get_account_info(&self, request: Request<ccd::GetAddressInfoRequest>) -> JsonResponse;

            fn get_birk_parameters(&self, request: Request<ccd::BlockHash>) -> JsonResponse;
        }
    }

    #[tonic::async_trait]
    impl ccd::p2p_server::P2p for MockService {
        async fn get_consensus_status(&self, request: Request<ccd::Empty>) -> JsonResponse {
            self.get_consensus_status(request)
        }

        async fn get_account_info(
            &self,
            request: Request<ccd::GetAddressInfoRequest>,
        ) -> JsonResponse {
            self.get_account_info(request)
        }

        async fn get_birk_parameters(&self, request: Request<ccd::BlockHash>) -> JsonResponse {
            self.get_birk_parameters(request)
        }
    }

    async fn init(srvc: MockService) -> std::io::Result<DynNodeClient> {
        let svc = ccd::p2p_server::P2pServer::new(srvc);

        let socket = tokio::net::TcpSocket::new_v6()?;
        socket.bind("[::1]:0".parse().unwrap())?;

        let port = socket.local_addr()?.port();

        let listener = tokio_stream::wrappers::TcpListenerStream::new(socket.listen(1)?);

        tokio::spawn(async {
            tonic::transport::Server::builder()
                .add_service(svc)
                .serve_with_incoming(listener)
                .await
                .unwrap();
        });

        Ok(Client::new(&format!("http://[::1]:{}", port)))
    }

    #[tokio::test]
    async fn test_get_last_block() {
        let mut service = MockService::default();

        service
            .expect_get_consensus_status()
            .times(1)
            .returning(|_| {
                Ok(Response::new(ccd::JsonResponse {
                    value: r#"{"lastFinalizedBlock": ":hash:"}"#.to_string(),
                }))
            });

        let client = init(service).await.unwrap();

        let res = client.get_last_block().await;

        assert!(matches!(res, Ok(hash) if hash == ":hash:"),);
    }

    #[tokio::test]
    async fn test_get_balances() {
        let mut service = MockService::default();

        service
            .expect_get_account_info()
            .withf(|arg| arg.get_ref().block_hash == "hash" && arg.get_ref().address == "addr")
            .times(1)
            .returning(move |_| {
                Ok(Response::new(ccd::JsonResponse {
                    value: r#"{
                    "accountNonce": 1,
                    "accountAmount": "256",
                    "accountIndex": 2,
                    "accountAddress": "address",
                    "accountBaker": {
                        "stakedAmount": "12.5",
                        "restakeEarnings": true,
                        "bakerId": 42
                    }
                }"#
                    .to_string(),
                }))
            });

        let client = init(service).await.unwrap();

        let res = client.get_balances("hash", "addr").await.unwrap();

        assert_eq!("243.5", res.0.to_string());
        assert_eq!("12.5", res.1.to_string());
    }

    #[tokio::test]
    async fn test_get_balances_no_network() {
        let client = Client::new("http://[::1]:8888");

        let res = client.get_balances("hash", "addr").await;

        assert!(matches!(res, Err(Error::Grpc(_))));
    }

    #[tokio::test]
    async fn test_get_balances_bad_json() {
        let mut service = MockService::default();

        service
            .expect_get_account_info()
            .times(1)
            .returning(move |_| {
                Ok(Response::new(ccd::JsonResponse {
                    value: "null".to_string(),
                }))
            });

        let client = init(service).await.unwrap();

        let res = client.get_balances("hash", "addr").await;

        assert!(matches!(res, Err(Error::Json(_))));
    }

    #[tokio::test]
    async fn test_get_baker() {
        let mut service = MockService::default();

        service
            .expect_get_birk_parameters()
            .withf(|request| request.get_ref().block_hash == ":hash:")
            .times(1)
            .returning(|_| {
                Ok(Response::new(ccd::JsonResponse {
                    value: r#"{
                    "bakers": [{
                        "bakerAccount": ":address:",
                        "bakerId": 1,
                        "bakerLotteryPower": 0.02
                    }]
                }"#
                    .to_string(),
                }))
            });

        let client = init(service).await.unwrap();

        let res = client.get_baker(":hash:", ":address:").await;

        assert!(matches!(res, Ok(baker) if matches!(
            &baker,
            Some(baker) if baker.id == 1,
        )));
    }

    #[tokio::test]
    async fn test_get_baker_not_found() {
        let mut service = MockService::default();

        service
            .expect_get_birk_parameters()
            .withf(|request| request.get_ref().block_hash == ":hash:")
            .times(1)
            .returning(|_| {
                Ok(Response::new(ccd::JsonResponse {
                    value: r#"{"bakers":[]}"#.to_string(),
                }))
            });

        let client = init(service).await.unwrap();

        let res = client.get_baker(":hash:", ":address:").await;

        assert!(matches!(res, Ok(baker) if matches!(&baker, None)));
    }
}
