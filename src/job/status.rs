use super::{AppError, AsyncJob};
use crate::client::DynNodeClient;
use crate::repository::status::{NewStatus, NodeStatusJson, ResourceStatusJson};
use crate::repository::DynStatusRepository;
use chrono::Utc;
use log::error;
use std::io;
use std::time::Duration;
use systemstat::{Platform, System};
use tokio::time;

const MAX_NUM_REPORT: i64 = 10_000;

/// A job to create a report of the status of the server and the blockchain
/// node.
pub struct StatusChecker {
    repository: DynStatusRepository,
    client: DynNodeClient,
    sleep_duration: Duration,
}

impl StatusChecker {
    pub fn new(repository: DynStatusRepository, client: DynNodeClient) -> Self {
        Self {
            repository,
            client,
            sleep_duration: Duration::from_millis(10_000),
        }
    }

    /// It gathers resource usage for the system and return the result. The call
    /// will sleep for some time to gather the CPU load.
    async fn get_system_stats(&self) -> ResourceStatusJson {
        let system = System::new();

        let memory = match system.memory() {
            Ok(m) => Some((m.free.0, m.total.0)),
            Err(e) => {
                error!("unable to gather memory usage: {}", e);
                None
            }
        };

        ResourceStatusJson {
            avg_cpu_load: match self.gather_cpu_load(&system).await {
                Ok(load) => Some(load),
                Err(e) => {
                    error!("unable to gather CPU load: {}", e);
                    None
                }
            },
            mem_free: memory.map(|m| m.0),
            mem_total: memory.map(|m| m.1),
            uptime_secs: match system.uptime() {
                Ok(uptime) => Some(uptime.as_secs()),
                Err(e) => {
                    error!("unable to gather uptime: {}", e);
                    None
                }
            },
        }
    }

    /// It sleeps for 10 seconds to gather the average load of the CPU.
    async fn gather_cpu_load(&self, system: &System) -> io::Result<f32> {
        let cpu = system.cpu_load_aggregate()?;

        // Sleep enough time to get an average load representative of the current
        // load of the system.
        time::sleep_until(time::Instant::now() + self.sleep_duration).await;

        let res = cpu.done()?;

        Ok(res.user + res.nice + res.system + res.interrupt)
    }

    /// It fetches the node for multiple statistics and build the JSON that will be
    /// store for the node status.
    async fn get_node_status(&self) -> Result<NodeStatusJson, AppError> {
        let node_info = self.client.get_node_info().await?;

        let uptime = self.client.get_node_uptime().await?;

        let node_stats = self.client.get_node_stats().await?;

        Ok(NodeStatusJson {
            node_id: node_info.node_id,
            baker_id: node_info.baker_id,
            is_baker_committee: node_info.is_baker_committee,
            is_finalizer_committee: node_info.is_finalizer_committee,
            uptime_ms: uptime,
            peer_type: node_info.peer_type,
            peer_average_latency: node_stats.avg_latency,
            peer_count: node_stats.peer_count,
        })
    }
}

#[async_trait]
impl AsyncJob for StatusChecker {
    async fn execute(&self) -> Result<(), AppError> {
        let new_status = NewStatus {
            resources: self.get_system_stats().await,
            node: match self.get_node_status().await {
                Ok(n) => Some(n),
                Err(_) => None,
            },
            timestamp_ms: Utc::now().timestamp_millis(),
        };

        self.repository.report(new_status).await?;

        // Keep only the most recent reports to avoid filling up the storage
        // indefinitely.
        self.repository.garbage_collect(MAX_NUM_REPORT).await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client::node::{MockNodeClient, NodeInfo, NodeStats};
    use crate::repository::status::MockStatusRepository;
    use std::sync::Arc;
    use mockall::predicate::*;

    #[tokio::test]
    async fn test_execute() {
        let mut client = MockNodeClient::new();

        client.expect_get_node_info().times(1).returning(|| {
            Ok(NodeInfo {
                node_id: None,
                baker_id: None,
                is_baker_committee: true,
                is_finalizer_committee: false,
                peer_type: "Node".to_string(),
            })
        });

        client
            .expect_get_node_uptime()
            .times(1)
            .returning(|| Ok(250));

        client.expect_get_node_stats().times(1).returning(|| {
            Ok(NodeStats {
                avg_latency: 125.75,
                avg_bps_in: 0,
                avg_bps_out: 0,
                peer_count: 6,
            })
        });

        let mut repository = MockStatusRepository::new();

        repository
            .expect_report()
            .withf(|status| !status.node.is_none())
            .times(1)
            .returning(|_| Ok(()));

        repository
            .expect_garbage_collect()
            .with(eq(MAX_NUM_REPORT))
            .times(1)
            .returning(|_| Ok(()));

        let mut job = StatusChecker::new(Arc::new(repository), Arc::new(client));

        // Sleep only for a short amount of time for the test.
        job.sleep_duration = Duration::from_millis(1);

        let res = job.execute().await;

        assert!(matches!(res, Ok(_)));
    }
}
