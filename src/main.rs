use axum::{extract::Extension, routing::get, Router};
use env_logger::Env;
use std::collections::HashMap;
use std::fs::File;
use std::str::FromStr;

use crate::config::Config;
use crate::job::Jobber;
use crate::repository::{account::SqliteAccountRepository, AsyncPool};

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate diesel_migrations;

#[macro_use]
extern crate async_trait;

mod client;
mod config;
mod controller;
mod job;
mod model;
mod repository;
mod schema;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    let mut file = File::open("./config.yaml")?;
    let cfg = Config::from_reader(&mut file)?;

    #[cfg(not(test))]
    let termination = async {
        tokio::signal::ctrl_c().await.unwrap();
    };

    // when testing the main function, we stop the server right away.
    #[cfg(test)]
    let termination = async {};

    run_server(cfg, termination).await;

    Ok(())
}

async fn prepare_jobs(jobs: &HashMap<config::Job, String>) -> Jobber {
    let mut scheduler = job::Scheduler::new();

    let price_client = client::bitfinex::BitfinexClient::default();

    for (name, schedule_str) in jobs {
        let schedule = cron::Schedule::from_str(schedule_str).unwrap();

        scheduler.register(
            name.as_str(),
            schedule,
            match name {
                config::Job::AccountsRefresher => Box::new(job::account::RefreshAccountsJob::new()),
                config::Job::PriceRefresher => {
                    Box::new(job::price::PriceRefresher::new(price_client.clone()))
                }
            },
        );
    }

    scheduler.start()
}

/// It creates an application and registers the different routes.
async fn create_app(pool: AsyncPool) -> Router {
    // Always run the migration to make sure the application is ready to use the
    // storage.
    pool.run_migrations().await.unwrap();

    let repo = SqliteAccountRepository::new(pool);

    Router::new()
        .route("/", get(controller::status))
        .route("/accounts/:addr", get(controller::get_account))
        .layer(Extension(repo))
}

/// It schedules the different jobs from the configuration and start the server.
/// The binding address is defined by the configuration.
async fn run_server(cfg: Config, termination: impl std::future::Future<Output = ()>) {
    let jobber = prepare_jobs(cfg.get_jobs()).await;

    let pool = AsyncPool::new("data.db");

    axum::Server::bind(cfg.get_listen_addr())
        .serve(create_app(pool).await.into_make_service())
        .with_graceful_shutdown(termination)
        .await
        .unwrap();

    // Shutdown the job controller.
    jobber.shutdown().await;
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use std::env;
    use tower::ServiceExt;

    /// It makes sure that the main function is running properly.
    #[test]
    fn test_main() {
        // Disable the logs for the test.
        env::set_var("RUST_LOG", "error");

        main().unwrap();
    }

    /// It makes sure that known job can be scheduled without error.
    #[tokio::test]
    async fn test_prepare_jobs() {
        let mut jobs = HashMap::new();
        jobs.insert(
            config::Job::AccountsRefresher,
            "* * * * * * 1970".to_string(),
        );

        let jobber = prepare_jobs(&jobs).await;

        jobber.shutdown().await;
    }

    /// It makes sure the server can bind the address.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_server() {
        let cfg = Config::default();

        run_server(cfg, async {}).await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_account() {
        let pool = AsyncPool::new(":memory:");

        let app = create_app(pool).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/accounts/:address:")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
