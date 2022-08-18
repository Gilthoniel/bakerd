use super::{DynNodeClient, Result};
use ccd::p2p_client::P2pClient;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::str::FromStr;
use std::sync::Arc;
use tonic::codegen::InterceptedService;
use tonic::metadata::Ascii;
use tonic::service::Interceptor;
use tonic::transport::Uri;
use tonic::{metadata::MetadataValue, transport::Channel, Request, Status};

mod ccd {
    tonic::include_proto!("concordium");
}

#[derive(Debug)]
pub struct Block {
    pub hash: String,
    pub height: i64,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BlockInfo {
    pub block_hash: String,
    pub block_height: i64,
    pub finalized: bool,
    pub block_baker: Option<i64>,

    #[serde(with = "serde_with::rust::display_fromstr")]
    pub block_slot_time: DateTime<Utc>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AccountBaker {
    pub staked_amount: Decimal,
    pub restake_earnings: bool,
    pub baker_id: u32,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    pub account_nonce: u32,
    pub account_amount: Decimal,
    pub account_index: u32,
    pub account_address: String,
    pub account_baker: Option<AccountBaker>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Baker {
    pub baker_account: String,
    pub baker_id: u64,
    pub baker_lottery_power: f64,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct BirkParameters {
    bakers: Vec<Baker>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ConsensusInfo {
    last_finalized_block: String,
    last_finalized_block_height: i64,
}

#[async_trait]
pub trait NodeClient {
    async fn get_last_block(&self) -> Result<Block>;

    async fn get_block_at_height(&self, height: i64) -> Result<Option<String>>;

    async fn get_block_info(&self, block_hash: &str) -> Result<BlockInfo>;

    async fn get_account_info(&self, block: &str, address: &str) -> Result<AccountInfo>;

    async fn get_baker(&self, block: &str, address: &str) -> Result<Option<Baker>>;
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
    async fn get_last_block(&self) -> Result<Block> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::Empty {});

        let response = client.get_consensus_status(request).await?.into_inner();

        let info: ConsensusInfo = serde_json::from_str(response.value.as_str())?;

        let block = Block {
            hash: info.last_finalized_block,
            height: info.last_finalized_block_height,
        };

        Ok(block)
    }

    async fn get_block_at_height(&self, height: i64) -> Result<Option<String>> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::BlockHeight {
            block_height: u64::try_from(height).unwrap(),
            from_genesis_index: 0,
            restrict_to_genesis_index: false,
        });

        let response = client.get_blocks_at_height(request).await?.into_inner();

        let mut hashes: Vec<String> = serde_json::from_str(&response.value)?;

        Ok(hashes.pop())
    }

    async fn get_block_info(&self, block_hash: &str) -> Result<BlockInfo> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::BlockHash {
            block_hash: block_hash.into(),
        });

        let response = client.get_block_info(request).await?.into_inner();

        Ok(serde_json::from_str(&response.value)?)
    }

    /// It returns the details of the account like its balance and the staked
    /// amount.
    async fn get_account_info(&self, block_hash: &str, address: &str) -> Result<AccountInfo> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::GetAddressInfoRequest {
            block_hash: String::from(block_hash),
            address: String::from(address),
        });

        let response = client.get_account_info(request).await?.into_inner();

        Ok(serde_json::from_str(response.value.as_str())?)
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
                return Ok(Some(baker));
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
        pub fn get_last_block(&self) -> Result<Block>;
        pub fn get_block_at_height(&self, height: i64) -> Result<Option<String>>;
        pub fn get_block_info(&self, block_hash: &str) -> Result<BlockInfo>;
        pub fn get_account_info(&self, block: &str, address: &str) -> Result<AccountInfo>;
        pub fn get_baker(&self, block: &str, address: &str) -> Result<Option<Baker>>;
    }
}

#[cfg(test)]
#[async_trait]
impl NodeClient for MockNodeClient {
    async fn get_last_block(&self) -> Result<Block> {
        self.get_last_block()
    }

    async fn get_block_at_height(&self, height: i64) -> Result<Option<String>> {
        self.get_block_at_height(height)
    }

    async fn get_block_info(&self, block_hash: &str) -> Result<BlockInfo> {
        self.get_block_info(block_hash)
    }

    async fn get_account_info(&self, block: &str, address: &str) -> Result<AccountInfo> {
        self.get_account_info(block, address)
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

            fn get_blocks_at_height(&self, request: Request<ccd::BlockHeight>) -> JsonResponse;

            fn get_block_info(&self, request: Request<ccd::BlockHash>) -> JsonResponse;

            fn get_account_info(&self, request: Request<ccd::GetAddressInfoRequest>) -> JsonResponse;

            fn get_birk_parameters(&self, request: Request<ccd::BlockHash>) -> JsonResponse;
        }
    }

    #[tonic::async_trait]
    impl ccd::p2p_server::P2p for MockService {
        async fn get_consensus_status(&self, request: Request<ccd::Empty>) -> JsonResponse {
            self.get_consensus_status(request)
        }

        async fn get_blocks_at_height(&self, request: Request<ccd::BlockHeight>) -> JsonResponse {
            self.get_blocks_at_height(request)
        }

        async fn get_block_info(&self, request: Request<ccd::BlockHash>) -> JsonResponse {
            self.get_block_info(request)
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
                    value: r#"{
                        "lastFinalizedBlock": ":hash:",
                        "lastFinalizedBlockHeight": 123
                    }"#
                    .to_string(),
                }))
            });

        let client = init(service).await.unwrap();

        let res = client.get_last_block().await;

        assert!(matches!(res, Ok(block) if block.hash == ":hash:"),);
    }

    #[tokio::test]
    async fn test_get_account_info() {
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

        let res = client.get_account_info("hash", "addr").await.unwrap();

        assert_eq!("256", res.account_amount.to_string());
        assert_eq!("12.5", res.account_baker.unwrap().staked_amount.to_string());
    }

    #[tokio::test]
    async fn test_get_account_info_no_network() {
        let client = Client::new("http://[::1]:8888");

        let res = client.get_account_info("hash", "addr").await;

        assert!(matches!(res, Err(Error::Grpc(_))));
    }

    #[tokio::test]
    async fn test_get_account_info_bad_json() {
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

        let res = client.get_account_info("hash", "addr").await;

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
            Some(baker) if baker.baker_id == 1,
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
