use rust_decimal::Decimal;
use serde::Deserialize;
use tonic::codegen::InterceptedService;
use tonic::metadata::Ascii;
use tonic::service::Interceptor;
use tonic::{metadata::MetadataValue, transport::Channel, Request, Status};
use tonic::transport::Uri;

use ccd::p2p_client::P2pClient;

pub mod ccd {
    tonic::include_proto!("concordium");
}

#[derive(Debug)]
pub enum Error {
    Status(tonic::Status),
    Json(serde_json::Error),
}

impl From<tonic::Status> for Error {
    fn from(e: tonic::Status) -> Self {
        Self::Status(e)
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct AccountBaker {
    staked_amount: Decimal,
    restake_earnings: bool,
    baker_id: u32,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo {
    account_nonce: u32,
    account_amount: Decimal,
    account_index: u32,
    account_address: String,
    account_baker: AccountBaker,
}

#[derive(Clone)]
pub struct Client {
    client: P2pClient<InterceptedService<Channel, Authorization>>,
}

impl Client {
    pub fn new(addr: Uri) -> Self {
        let channel = Channel::builder(addr).connect_lazy();

        let client = P2pClient::with_interceptor(channel, Authorization::new());

        Client {
            client,
        }
    }

    /// Return the details of the account like its balance and the staked amount.
    pub async fn get_account_info(
        &mut self,
        block_hash: &str,
        address: &str,
    ) -> Result<AccountInfo, Error> {
        let request = Request::new(ccd::GetAddressInfoRequest {
            block_hash: String::from(block_hash),
            address: String::from(address),
        });

        let response = self.client.get_account_info(request).await?.into_inner();

        let info = serde_json::from_str(response.value.as_str())?;

        Ok(info)
    }
}

#[derive(Clone)]
struct Authorization {
    token: MetadataValue<Ascii>,
}

impl Authorization {
    fn new() -> Self {
        let token = MetadataValue::try_from("rpcadmin")
            .expect("authorization token is malformed");

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
mod integration_tests {
    use super::*;
    use tonic::{Request, Response, Status};

    struct Service;

    #[tonic::async_trait]
    impl ccd::p2p_server::P2p for Service {
        async fn get_account_info(
            &self,
            _: Request<ccd::GetAddressInfoRequest>,
        ) -> Result<Response<ccd::JsonResponse>, Status> {
            let value = r#"{
                "accountNonce": 1,
                "accountAmount": "256",
                "accountIndex": 2,
                "accountAddress": "address",
                "accountBaker": {
                    "stakedAmount": "12.123",
                    "restakeEarnings": true,
                    "bakerId": 42
                }
            }"#.to_string();

            Ok(Response::new(ccd::JsonResponse { value }))
        }
    }

    async fn init() -> std::io::Result<u16> {
        let svc = ccd::p2p_server::P2pServer::new(Service);

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

        Ok(port)
    }

    #[tokio::test]
    async fn test_get_account_info() {
        let port = init().await.unwrap();

        let mut client = Client::new(format!("http://[::1]:{}", port).parse::<Uri>().unwrap());

        let res = client.get_account_info("hash", "addr").await.unwrap();
        assert_eq!(1, res.account_nonce);
    }
}
