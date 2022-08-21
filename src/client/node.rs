use super::{DynNodeClient, Result};
use ccd::node_info_response::IsInBakingCommittee;
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

#[derive(Debug)]
pub struct NodeInfo {
    pub node_id: Option<String>,
    pub baker_id: Option<u64>,
    pub is_baker_committee: bool,
    pub is_finalizer_committee: bool,
    pub peer_type: String,
}

#[derive(Debug)]
pub struct NodeStats {
    pub avg_latency: f64,
    pub avg_bps_in: u64,
    pub avg_bps_out: u64,
    pub peer_count: usize,
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
pub struct Event {
    pub tag: String,
    pub account: Option<String>,
    pub baker_reward: Option<Decimal>,
    pub transaction_fees: Option<Decimal>,
    pub finalization_reward: Option<Decimal>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct BlockSummary {
    pub special_events: Vec<Event>,
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

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait NodeClient {
    async fn get_node_info(&self) -> Result<NodeInfo>;

    async fn get_node_uptime(&self) -> Result<u64>;

    async fn get_node_stats(&self) -> Result<NodeStats>;

    async fn get_last_block(&self) -> Result<Block>;

    async fn get_block_at_height(&self, height: i64) -> Result<Option<String>>;

    async fn get_block_info(&self, block_hash: &str) -> Result<BlockInfo>;

    async fn get_block_summary(&self, block_hash: &str) -> Result<BlockSummary>;

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
    /// It returns the information about the status of the node.
    async fn get_node_info(&self) -> Result<NodeInfo> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::Empty {});

        let response = client.node_info(request).await?.into_inner();

        Ok(NodeInfo {
            node_id: response.node_id,
            baker_id: response.consensus_baker_id,
            is_baker_committee: is_in_baker_committee(response.consensus_baker_committee),
            is_finalizer_committee: response.consensus_finalizer_committee,
            peer_type: response.peer_type,
        })
    }

    /// It asks to the node for the uptime and the value in milliseconds.
    async fn get_node_uptime(&self) -> Result<u64> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::Empty {});

        let response = client.peer_uptime(request).await?.into_inner();

        Ok(response.value)
    }

    /// It returns the statistics of the blockchain node.
    async fn get_node_stats(&self) -> Result<NodeStats> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::PeersRequest {
            include_bootstrappers: false,
        });

        let response = client.peer_stats(request).await?.into_inner();

        Ok(NodeStats {
            avg_latency: compute_avg_latency(&response),
            avg_bps_in: response.avg_bps_in,
            avg_bps_out: response.avg_bps_out,
            peer_count: response.peerstats.len(),
        })
    }

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

    /// It returns the hash of the block at the given height. It expects only
    /// one hash per height but returns the first if multiple are found.
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

    /// It returns the information about the block with the given hash.
    async fn get_block_info(&self, block_hash: &str) -> Result<BlockInfo> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::BlockHash {
            block_hash: block_hash.into(),
        });

        let response = client.get_block_info(request).await?.into_inner();

        Ok(serde_json::from_str(&response.value)?)
    }

    /// It returns the summary of the block like special events.
    async fn get_block_summary(&self, block_hash: &str) -> Result<BlockSummary> {
        let mut client = self.client.clone();

        let request = Request::new(ccd::BlockHash {
            block_hash: block_hash.into(),
        });

        let response = client.get_block_summary(request).await?.into_inner();

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

fn is_in_baker_committee(value: i32) -> bool {
    value == i32::from(IsInBakingCommittee::ActiveInCommittee)
}

fn compute_avg_latency(stats: &ccd::PeerStatsResponse) -> f64 {
    let mut t = 0;

    for stat in &stats.peerstats {
        t += stat.latency;
    }

    let size = stats.peerstats.len() as f64;

    t as f64 / size
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::client::Error;
    use mockall::predicate::*;
    use tonic::{Request, Response, Status};

    type JsonResponse = std::result::Result<Response<ccd::JsonResponse>, Status>;
    type NumberResponse = std::result::Result<Response<ccd::NumberResponse>, Status>;
    type NodeInfoResponse = std::result::Result<Response<ccd::NodeInfoResponse>, Status>;
    type PeerStatsResponse = std::result::Result<Response<ccd::PeerStatsResponse>, Status>;

    mockall::mock! {
        pub Service {}

        #[async_trait]
        impl ccd::p2p_server::P2p for Service {
            async fn node_info(&self, request: Request<ccd::Empty>) -> NodeInfoResponse;
            async fn peer_uptime(&self, request: Request<ccd::Empty>) -> NumberResponse;
            async fn peer_stats(&self, request: Request<ccd::PeersRequest>) -> PeerStatsResponse;
            async fn get_consensus_status(&self, request: Request<ccd::Empty>) -> JsonResponse;
            async fn get_blocks_at_height(&self, request: Request<ccd::BlockHeight>) -> JsonResponse;
            async fn get_block_info(&self, request: Request<ccd::BlockHash>) -> JsonResponse;
            async fn get_block_summary(&self, request: Request<ccd::BlockHash>) -> JsonResponse;
            async fn get_account_info(&self, request: Request<ccd::GetAddressInfoRequest>) -> JsonResponse;
            async fn get_birk_parameters(&self, request: Request<ccd::BlockHash>) -> JsonResponse;
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
    async fn test_get_node_info() {
        let mut service = MockService::new();

        service.expect_node_info().times(1).returning(|_| {
            Ok(Response::new(ccd::NodeInfoResponse {
                node_id: Some("deadbeef".to_string()),
                current_localtime: 0,
                peer_type: "Node".to_string(),
                consensus_baker_running: true,
                consensus_running: true,
                consensus_type: "".to_string(),
                consensus_baker_committee: 3,
                consensus_finalizer_committee: true,
                consensus_baker_id: Some(42),
            }))
        });

        let client = init(service).await.unwrap();

        let res = client.get_node_info().await;

        assert!(matches!(res, Ok(_)));
    }

    #[tokio::test]
    async fn test_get_node_uptime() {
        let mut service = MockService::new();

        service
            .expect_peer_uptime()
            .times(1)
            .returning(|_| Ok(Response::new(ccd::NumberResponse { value: 42 })));

        let client = init(service).await.unwrap();

        let res = client.get_node_uptime().await;

        assert!(matches!(res, Ok(uptime) if uptime == 42));
    }

    #[tokio::test]
    async fn test_get_node_stats() {
        let mut service = MockService::new();

        service.expect_peer_stats().times(1).returning(|_| {
            Ok(Response::new(ccd::PeerStatsResponse {
                avg_bps_in: 0,
                avg_bps_out: 0,
                peerstats: vec![
                    ccd::peer_stats_response::PeerStats {
                        node_id: "peer-1".to_string(),
                        packets_sent: 0,
                        packets_received: 0,
                        latency: 200,
                    },
                    ccd::peer_stats_response::PeerStats {
                        node_id: "peer-2".to_string(),
                        packets_sent: 0,
                        packets_received: 0,
                        latency: 100,
                    },
                ],
            }))
        });

        let client = init(service).await.unwrap();

        let res = client.get_node_stats().await;

        assert!(matches!(res, Ok(node_stats) if node_stats.avg_latency == 150.0));
    }

    #[tokio::test]
    async fn test_get_last_block() {
        let mut service = MockService::new();

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
    async fn test_get_block_at_height() {
        let mut service = MockService::default();

        service
            .expect_get_blocks_at_height()
            .withf(|r| r.get_ref().block_height == 42)
            .times(1)
            .returning(|_| {
                Ok(Response::new(ccd::JsonResponse {
                    value: "[\":hash:\"]".to_string(),
                }))
            });

        let client = init(service).await.unwrap();

        let res = client.get_block_at_height(42).await;

        assert!(matches!(res, Ok(hash) if hash == Some(":hash:".to_string())));
    }

    #[tokio::test]
    async fn test_get_block_info() {
        let mut service = MockService::default();

        service
            .expect_get_block_info()
            .withf(|r| r.get_ref().block_hash == ":hash:")
            .times(1)
            .returning(|_| {
                Ok(Response::new(ccd::JsonResponse {
                    value: r#"{
                        "blockHash": ":hash:",
                        "blockHeight": 3,
                        "finalized": true,
                        "blockBaker": 42,
                        "blockSlotTime": "2022-08-19T09:03:40Z"
                    }"#
                    .to_string(),
                }))
            });

        let client = init(service).await.unwrap();

        let res = client.get_block_info(":hash:").await;

        assert!(matches!(res, Ok(info) if info.finalized));
    }

    #[tokio::test]
    async fn test_get_block_summary() {
        let mut service = MockService::default();

        service
            .expect_get_block_summary()
            .withf(|r| r.get_ref().block_hash == ":hash:")
            .times(1)
            .returning(|_| {
                Ok(Response::new(ccd::JsonResponse {
                    value: r#"{
                        "specialEvents": []
                    }"#
                    .to_string(),
                }))
            });

        let client = init(service).await.unwrap();

        let res = client.get_block_summary(":hash:").await;

        assert!(matches!(res, Ok(_)));
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
