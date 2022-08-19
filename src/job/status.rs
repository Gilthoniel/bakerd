use super::{AppError, AsyncJob};
use crate::repository::status::{NewStatus, ResourceStatusJson};
use crate::repository::DynStatusRepository;
use chrono::Utc;
use log::error;
use std::io;
use std::time::Duration;
use systemstat::{Platform, System};
use tokio::time;

/// A job to create a report of the status of the server and the blockchain
/// node.
pub struct StatusChecker {
    repository: DynStatusRepository,
}

impl StatusChecker {
    pub fn new(repository: DynStatusRepository) -> Self {
        Self { repository }
    }
}

#[async_trait]
impl AsyncJob for StatusChecker {
    async fn execute(&self) -> Result<(), AppError> {
        let new_status = NewStatus {
            resources: get_system_stats().await,
            timestamp_ms: Utc::now().timestamp_millis(),
        };

        self.repository.report(new_status).await?;

        Ok(())
    }
}

/// It gathers resource usage for the system and return the result. The call
/// will sleep for some time to gather the CPU load.
async fn get_system_stats() -> ResourceStatusJson {
    let system = System::new();

    let memory = match system.memory() {
        Ok(m) => Some((m.free.0, m.total.0)),
        Err(e) => {
            error!("unable to gather memory usage: {}", e);
            None
        }
    };

    ResourceStatusJson {
        avg_cpu_load: match gather_cpu_load(&system).await {
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
async fn gather_cpu_load(system: &System) -> io::Result<f32> {
    let cpu = system.cpu_load_aggregate()?;

    // Sleep enough time to get an average load representative of the current
    // load of the system.
    time::sleep_until(time::Instant::now() + Duration::from_millis(10000)).await;

    let res = cpu.done()?;

    Ok(res.user + res.nice + res.system + res.interrupt)
}
