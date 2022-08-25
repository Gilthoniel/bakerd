use crate::config::Config;
use crate::job::Jobber;
use crate::repository::*;
use axum::{extract::Extension, routing::get, Router};
use clap::Parser;
use env_logger::Env;
use std::collections::HashMap;
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
    user_repository: DynUserRepository,
}

impl Context {
    fn new(pool: &AsyncPool) -> Self {
        Self {
            account_repository: Arc::new(SqliteAccountRepository::new(pool.clone())),
            price_repository: Arc::new(SqlitePriceRepository::new(pool.clone())),
            block_repository: Arc::new(SqliteBlockRepository::new(pool.clone())),
            status_repository: Arc::new(SqliteStatusRepository::new(pool.clone())),
            user_repository: Arc::new(SqliteUserRepository::new(pool.clone())),
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

    #[clap(short, long, value_parser)]
    secret_file: Option<String>,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            config_file: "config.yaml".to_string(),
            data_dir: "data.db".to_string(),
            secret_file: None,
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    #[cfg(not(test))]
    let args = Args::parse();

    #[cfg(test)]
    let args = Args::default();

    let cfg = Config::from_file(&args.config_file)?;

    #[cfg(not(test))]
    let termination = async {
        tokio::signal::ctrl_c().await.unwrap();
    };

    // when testing the main function, we stop the server right away.
    #[cfg(test)]
    let termination = async {};

    run_server(&cfg, &args, termination).await.unwrap();

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

async fn prepare_context(data_dir: &str) -> std::result::Result<Context, PoolError> {
    let pool = AsyncPool::open(data_dir)?;

    // Always run the migration to make sure the application is ready to use the
    // storage.
    pool.run_migrations().await?;

    Ok(Context::new(&pool))
}

/// It creates an application and registers the different routes.
async fn create_app(ctx: &Context, cfg: &Config, args: &Args) -> Router {
    let secret = cfg
        .get_secret(args.secret_file.as_ref().map(|p| p.as_ref()))
        .expect("unable to read secret file");

    // as a better security, the secret is only cloned once and shared between
    // the requests.
    let secret = Arc::new(secret.clone());

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
        .layer(Extension(ctx.user_repository.clone()))
        .layer(axum::middleware::from_fn(move |req, next| {
            middleware::authentication(req, next, secret.clone())
        }))
}

/// It schedules the different jobs from the configuration and start the server.
/// The binding address is defined by the configuration.
async fn run_server(
    cfg: &Config,
    args: &Args,
    termination: impl std::future::Future<Output = ()>,
) -> std::result::Result<(), PoolError> {
    let ctx = prepare_context(&args.data_dir).await?;

    let jobber = prepare_jobs(cfg, &ctx).await;

    axum::Server::bind(cfg.get_listen_addr())
        .serve(create_app(&ctx, &cfg, args).await.into_make_service())
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
    use std::fs::File;
    use std::io::prelude::*;
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

        let pool = AsyncPool::open(":memory:").unwrap();

        let jobber = prepare_jobs(&cfg, &Context::new(&pool)).await;

        jobber.shutdown().await;
    }

    /// It makes sure the server can bind the address.
    #[tokio::test(flavor = "multi_thread")]
    async fn test_run_server() {
        let cfg = Config::default();
        let mut args = Args::default();
        args.data_dir = ":memory:".to_string();

        run_server(&cfg, &args, async {}).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_get_account() {
        let mut secret_file = env::temp_dir();
        secret_file.push("secret.txt");

        let mut file = File::create(secret_file.clone()).unwrap();
        file.write_all(b"IUBePnVgKXFPc2QzZTRuSykuQic5IUt8QlY=")
            .unwrap();

        let token = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJleHAiOjMyNTIzOTk5OTgxfQ.KLkRLSduNLILsUlOJewQ1ihhsefZZ6Ris9IwkQ7IZtU";

        let ctx = prepare_context(":memory:").await.unwrap();
        let mut args = Args::default();
        args.secret_file = secret_file.to_str().map(String::from);

        let app = create_app(&ctx, &Config::default(), &args).await;

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/accounts/:address:")
                    .header("Authorization", format!("Bearer {}", token))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
