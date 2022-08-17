pub mod account;
pub mod price;

use cron::Schedule;
use log::{error, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, Barrier};
use tokio::task::JoinHandle;
use tokio::time;

use crate::controller::AppError;

const SCHEDULER_TIMEOUT: Duration = Duration::from_millis(10000);

#[async_trait]
pub trait AsyncJob: Sync + Send {
    async fn execute(&self) -> Result<(), AppError>;
}

struct Context {
    name: String,
    closed: watch::Receiver<bool>,
    barrier: Arc<Barrier>,
    schedule: Schedule,
}

pub struct Scheduler {
    jobs: HashMap<String, (Schedule, Box<dyn AsyncJob>)>,
}

impl Scheduler {
    /// It creates a new job scheduler that is initially empty.
    pub fn new() -> Self {
        Self {
            jobs: HashMap::new(),
        }
    }

    /// It registers a job given a name and a schedule.
    pub fn register(&mut self, name: &str, schedule: Schedule, job: Box<dyn AsyncJob>) {
        self.jobs.insert(name.to_string(), (schedule, job));
    }

    /// It schedules the registered job and returns a controller that can be
    /// closed.
    pub fn start(self) -> Jobber {
        let (closer, closing) = watch::channel(false);

        let barrier = Arc::new(Barrier::new(self.jobs.len() + 1));

        for (name, (schedule, job)) in self.jobs {
            let ctx = Context {
                name: name.clone(),
                closed: closing.clone(),
                barrier: barrier.clone(),
                schedule: schedule,
            };

            Self::schedule_job(ctx, job);
        }

        let handle = tokio::spawn(async move {
            let mut closing = closing;

            closing
                .changed()
                .await
                .expect("unexpected error when closing");

            let timeout = tokio::time::sleep(SCHEDULER_TIMEOUT);

            // Either the job will all finish in the graceful period, or the timeout
            // will force stop.
            tokio::select! {
                _ = barrier.wait() => info!("all jobs have gracefully terminated"),
                _ = timeout => warn!("scheduler has timing out when terminating"),
            }
        });

        Jobber { closer, handle }
    }

    fn schedule_job(ctx: Context, job: Box<dyn AsyncJob>) {
        tokio::spawn(async move {
            info!("job {} has been scheduled", ctx.name);

            let mut closed = ctx.closed;

            loop {
                let instant = Self::sleep_schedule(&ctx.schedule);

                tokio::select! {
                    _ = time::sleep_until(instant) => {}
                    _ = closed.changed() => {
                        info!("job [{}] has stopped", ctx.name);

                        // Wait for all the jobs to finish.
                        ctx.barrier.wait().await;
                        return;
                    }
                }

                info!("job [{}] has started", ctx.name);

                match job.execute().await {
                    Ok(_) => info!("job [{}] has finished", ctx.name),
                    Err(e) => error!("job [{}] has finished with error {:?}", ctx.name, e),
                }
            }
        });
    }

    /// It returns a future that will return at the next schedule of the job. If
    /// the schedule cannot be determined, it will return right away.
    fn sleep_schedule(schedule: &Schedule) -> time::Instant {
        let next = schedule
            .upcoming(chrono::Utc)
            .next()
            .unwrap_or(chrono::Utc::now());

        let duration = (next - chrono::Utc::now())
            .to_std()
            .unwrap_or(Duration::from_millis(0));

        time::Instant::now() + duration
    }
}

pub struct Jobber {
    closer: watch::Sender<bool>,
    handle: JoinHandle<()>,
}

impl Jobber {
    pub async fn shutdown(self) {
        info!("job scheduler is shutting down");

        self.closer.send(true).unwrap();

        self.handle.await.unwrap();

        info!("job scheduler has been shutdown");
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use std::str::FromStr;
    use tokio::sync::watch;

    pub struct DummyJob {
        done: watch::Sender<bool>,
    }

    #[async_trait]
    impl AsyncJob for DummyJob {
        async fn execute(&self) -> Result<(), AppError> {
            self.done.send(true).unwrap();

            Ok(())
        }
    }

    #[tokio::test]
    async fn test_job_scheduling() {
        let mut scheduler = Scheduler::new();

        let (done, mut rx) = watch::channel(false);

        let schedule = cron::Schedule::from_str("* * * * * *").unwrap();

        scheduler.register("dummy", schedule, Box::new(DummyJob { done }));

        let jobber = scheduler.start();

        let res = rx.changed().await;
        assert_eq!(true, res.is_ok());

        jobber.shutdown().await;
    }
}
