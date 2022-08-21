use crate::config::Config;
use crate::controller::AppError;
use crate::job::Jobber;
use crate::repository::*;
use axum::{extract::Extension, routing::get, Router};
use clap::Parser;
use env_logger::Env;
use std::collections::HashMap;
use std::fs::File;
use std::str::FromStr;
use std::sync::Arc;

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
mod middleware;
mod model;
mod repository;
mod schema;

struct Context {
    account_repository: DynAccountRepository,
    price_repository: DynPriceRepository,
    block_repository: DynBlockRepository,
    status_repository: DynStatusRepository,
}

impl Context {
    fn new(pool: &AsyncPool) -> Self {
        Self {
            account_repository: Arc::new(SqliteAccountRepository::new(pool.clone())),
            price_repository: Arc::new(SqlitePriceRepository::new(pool.clone())),
            block_repository: Arc::new(SqliteBlockRepository::new(pool.clone())),
            status_repository: Arc::new(SqliteStatusRepository::new(pool.clone())),
        }
    }
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
    #[clap(short, long, value_parser, default_value = "config.yaml")]
    config_file: String,

    #[clap(short, long, value_parser, default_value = "data.db")]
    data_dir: String,
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    #[cfg(not(test))]
    let args = Args::parse();

    #[cfg(test)]
    let args = Args {
        config_file: "config.yaml".to_string(),
        data_dir: "data.db".to_string(),
    };

    let mut file = File::open(args.config_file)?;
    let cfg = Config::from_reader(&mut file)?;

    #[cfg(not(test))]
    let termination = async {
        tokio::signal::ctrl_c().await.unwrap();
    };

    // when testing the main function, we stop the server right away.
    #[cfg(test)]
    let termination = async {};

    run_server(&cfg, &args.data_dir, termination).await.unwrap();

    Ok(())
}

async fn prepare_jobs(cfg: &Config, ctx: &Context) -> Jobber {
    let mut scheduler = job::Scheduler::new();

    let price_client = client::bitfinex::BitfinexClient::default();

    let node_client = client::node::Client::new("http://127.0.0.1:10000");

    for (name, schedule_str) in cfg.get_jobs().unwrap_or(&HashMap::new()) {
        let schedule = cron::Schedule::from_str(&schedule_str).unwrap();

        scheduler.register(
            name.as_str(),
            schedule,
            match name {
                config::Job::AccountsRefresher => {
                    let mut job = job::account::RefreshAccountsJob::new(
                        node_client.clone(),
                        ctx.account_repository.clone(),
                    );

                    for address in cfg.get_accounts().unwrap_or(&vec![]) {
                        job.follow_account(address);
                    }

                    Box::new(job)
                }
                config::Job::PriceRefresher => {
                    let mut job = job::price::PriceRefresher::new(
                        price_client.clone(),
                        ctx.price_repository.clone(),
                    );

                    for pair in cfg.get_pairs().unwrap_or(&vec![]) {
                        job.follow_pair(pair.clone());
                    }

                    Box::new(job)
                }
                config::Job::BlockFetcher => {
                    let mut job = job::block::BlockFetcher::new(
                        node_client.clone(),
                        ctx.block_repository.clone(),
                        ctx.account_repository.clone(),
                    );

                    for address in cfg.get_accounts().unwrap_or(&vec![]) {
                        job.follow_account(address);
                    }

                    Box::new(job)
                }
                config::Job::StatusChecker => Box::new(job::status::StatusChecker::new(
                    ctx.status_repository.clone(),
                    node_client.clone(),
                )),
            },
        );
    }

    scheduler.start()
}

async fn prepare_context(data_dir: &str) -> std::result::Result<Context, AppError> {
    let pool = AsyncPool::new(data_dir);

    // Always run the migration to make sure the application is ready to use the
    // storage.
    pool.run_migrations().await?;

    Ok(Context::new(&pool))
}

/// It creates an application and registers the different routes.
async fn create_app(ctx: &Context, cfg: &Config) -> Router {
    // as a better security, the secret is only cloned once and shared between
    // the requests.
    let secret = Arc::new(cfg.get_secret().clone());

    Router::new()
        .route("/", get(controller::get_status))
        .route("/accounts/:addr", get(controller::get_account))
        .route(
            "/accounts/:addr/rewards",
            get(controller::get_account_rewards),
        )
        .route("/prices/:pair", get(controller::get_price))
        .layer(Extension(ctx.account_repository.clone()))
        .layer(Extension(ctx.price_repository.clone()))
        .layer(Extension(ctx.block_repository.clone()))
        .layer(Extension(ctx.status_repository.clone()))
        .layer(axum::middleware::from_fn(move |req, next| {
            middleware::authentication(req, next, secret.clone())
        }))
}

/// It schedules the different jobs from the configuration and start the server.
/// The binding address is defined by the configuration.
async fn run_server(
    cfg: &Config,
    data_dir: &str,
    termination: impl std::future::Future<Output = ()>,
) -> std::result::Result<(), AppError> {
    let ctx = prepare_context(data_dir).await?;

    let jobber = prepare_jobs(cfg, &ctx).await;

    axum::Server::bind(cfg.get_listen_addr())
        .serve(create_app(&ctx, &cfg).await.into_make_service())
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
            "secret: \"abc\"\n",
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

        run_server(&cfg, ":memory:", async {}).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_account() {
        let ctx = prepare_context(":memory:").await.unwrap();

        let app = create_app(&ctx, &Config::default()).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/accounts/:address:")
                    .header("Authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
