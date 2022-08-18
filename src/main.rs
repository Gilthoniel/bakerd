use axum::{extract::Extension, routing::get, Router};
use env_logger::Env;
use std::fs::File;
use std::str::FromStr;

use crate::config::Config;
use crate::job::Jobber;
use crate::repository::{
    account::SqliteAccountRepository as AccountRepository,
    price::SqlitePriceRepository as PriceRepository, AsyncPool,
};
use controller::AppError;

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

struct Context {
    account_repository: repository::account::DynAccountRepository,
    price_repository: repository::price::DynPriceRepository,
}

impl Context {
    fn new(pool: &AsyncPool) -> Self {
        Self {
            account_repository: AccountRepository::new(pool.clone()),
            price_repository: PriceRepository::new(pool.clone()),
        }
    }
}

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

    run_server(&cfg, termination).await.unwrap();

    Ok(())
}

async fn prepare_jobs(cfg: &Config, ctx: &Context) -> Jobber {
    let mut scheduler = job::Scheduler::new();

    let price_client = client::bitfinex::BitfinexClient::default();

    let node_client = client::node::Client::new("http://127.0.0.1:10000");

    for (name, schedule_str) in cfg.get_jobs() {
        let schedule = cron::Schedule::from_str(schedule_str).unwrap();

        scheduler.register(
            name.as_str(),
            schedule,
            match name {
                config::Job::AccountsRefresher => {
                    Box::new(job::account::RefreshAccountsJob::new(node_client.clone()))
                },
                config::Job::PriceRefresher => {
                    let mut job = job::price::PriceRefresher::new(
                        price_client.clone(),
                        ctx.price_repository.clone(),
                    );

                    for pair in cfg.get_pairs() {
                        job.follow_pair(pair.clone());
                    }

                    Box::new(job)
                }
            },
        );
    }

    scheduler.start()
}

async fn prepare_context() -> Result<Context, AppError> {
    let pool = AsyncPool::new("data.db");

    // Always run the migration to make sure the application is ready to use the
    // storage.
    pool.run_migrations().await?;

    Ok(Context::new(&pool))
}

/// It creates an application and registers the different routes.
async fn create_app(ctx: &Context) -> Router {
    Router::new()
        .route("/", get(controller::status))
        .route("/accounts/:addr", get(controller::get_account))
        .route("/prices/:pair", get(controller::get_price))
        .layer(Extension(ctx.account_repository.clone()))
        .layer(Extension(ctx.price_repository.clone()))
}

/// It schedules the different jobs from the configuration and start the server.
/// The binding address is defined by the configuration.
async fn run_server(
    cfg: &Config,
    termination: impl std::future::Future<Output = ()>,
) -> Result<(), AppError> {
    let ctx = prepare_context().await?;

    let jobber = prepare_jobs(cfg, &ctx).await;

    axum::Server::bind(cfg.get_listen_addr())
        .serve(create_app(&ctx).await.into_make_service())
        .with_graceful_shutdown(termination)
        .await
        .unwrap();

    // Shutdown the job controller.
    jobber.shutdown().await;

    Ok(())
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
        let values = concat!(
            "listen_address: 127.0.0.1:8080\n",
            "jobs:\n",
            "  accounts_refresher: \"* * * * * * 1970\"\n",
            "  price_refresher: \"* * * * * * 1970\"\n",
            "pairs:\n",
            "  - [\"BTC\", \"USD\"]\n"
        );

        let mut values = values.as_bytes();

        let cfg = Config::from_reader(&mut values).unwrap();

        let jobber = prepare_jobs(&cfg, &Context::new(&AsyncPool::new(":memory:"))).await;

        jobber.shutdown().await;
    }

    /// It makes sure the server can bind the address.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_server() {
        let cfg = Config::default();

        run_server(&cfg, async {}).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_account() {
        let ctx = prepare_context().await.unwrap();

        let app = create_app(&ctx).await;

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
