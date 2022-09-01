mod authentication;
mod client;
mod config;
mod controller;
mod job;
mod model;
mod repository;
mod schema;

use crate::client::bitfinex;
use crate::config::Config;
use crate::job::Jobber;
use crate::repository::*;
use axum::{
  extract::Extension,
  routing::{get, post},
  Router,
};
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

// A result type alias for the main application.
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

// Command-line arguments of the application.
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

struct Dependencies {
  args: Args,
  cfg: Config,
  account: DynAccountRepository,
  price: DynPriceRepository,
  block: DynBlockRepository,
  status: DynStatusRepository,
  user: DynUserRepository,
}

impl Dependencies {
  async fn make(args: Args, cfg: Config) -> Result<Self> {
    let pool = AsyncPool::open(&args.data_dir)?;

    // Always run the migration to make sure the application is ready to use the
    // storage.
    pool.run_migrations().await?;

    Ok(Dependencies {
      args,
      cfg,
      account: Arc::new(SqliteAccountRepository::new(pool.clone())),
      price: Arc::new(SqlitePriceRepository::new(pool.clone())),
      block: Arc::new(SqliteBlockRepository::new(pool.clone())),
      status: Arc::new(SqliteStatusRepository::new(pool.clone())),
      user: Arc::new(SqliteUserRepository::new(pool.clone())),
    })
  }
}

#[tokio::main]
async fn main() -> Result<()> {
  env_logger::init_from_env(Env::default().default_filter_or("info"));

  #[cfg(not(test))]
  let args = Args::parse();

  #[cfg(test)]
  let args = Args::default();

  let cfg = Config::from_file(&args.config_file)?;

  let deps = Dependencies::make(args, cfg).await?;

  #[cfg(not(test))]
  let termination = async {
    tokio::signal::ctrl_c().await.unwrap();
  };

  // when testing the main function, we stop the server right away.
  #[cfg(test)]
  let termination = async {};

  run_server(&deps, termination).await?;

  Ok(())
}

async fn prepare_jobs(deps: &Dependencies) -> Jobber {
  let mut scheduler = job::Scheduler::new();

  let price_client = bitfinex::BitfinexClient::default();

  let node_client = deps.cfg.make_client();

  for (name, schedule_str) in deps.cfg.get_jobs().unwrap_or(&HashMap::new()) {
    let schedule = cron::Schedule::from_str(&schedule_str).unwrap();

    scheduler.register(
      name.as_str(),
      schedule,
      match name {
        config::Job::AccountsRefresher => Box::new(job::account::RefreshAccountsJob::new(
          node_client.clone(),
          deps.account.clone(),
        )),
        config::Job::PriceRefresher => {
          let mut job = job::price::PriceRefresher::new(price_client.clone(), deps.price.clone());

          for pair in deps.cfg.get_pairs().unwrap_or(&vec![]) {
            job.follow_pair(pair.clone());
          }

          Box::new(job)
        }
        config::Job::BlockFetcher => Box::new(job::block::BlockFetcher::new(
          node_client.clone(),
          deps.block.clone(),
          deps.account.clone(),
        )),
        config::Job::StatusChecker => Box::new(job::status::StatusChecker::new(
          deps.status.clone(),
          node_client.clone(),
        )),
      },
    );
  }

  scheduler.start()
}

/// It creates an application and registers the different routes.
async fn create_app(deps: &Dependencies) -> Result<Router> {
  let secret_file = deps.args.secret_file.as_ref().map(|p| p.as_ref());

  let decoding_key = deps.cfg.get_decoding_key(secret_file)?;
  let encoding_key = deps.cfg.get_encoding_key(secret_file)?;

  Ok(
    Router::new()
      .route("/", get(controller::get_status))
      .route("/auth/authorize", post(controller::auth::authorize))
      .route("/auth/token", post(controller::auth::refresh_token))
      .route("/users", post(controller::auth::create_user))
      .route("/accounts", post(controller::create_account))
      .route("/accounts/:addr", get(controller::get_account))
      .route("/accounts/:addr/rewards", get(controller::get_account_rewards))
      .route("/prices/:pair", get(controller::get_price))
      .route("/blocks", get(controller::get_blocks))
      .layer(Extension(deps.account.clone()))
      .layer(Extension(deps.price.clone()))
      .layer(Extension(deps.block.clone()))
      .layer(Extension(deps.status.clone()))
      .layer(Extension(deps.user.clone()))
      .layer(Extension(Arc::new(encoding_key)))
      .layer(Extension(Arc::new(decoding_key))),
  )
}

/// It schedules the different jobs from the configuration and start the server.
/// The binding address is defined by the configuration.
async fn run_server(deps: &Dependencies, termination: impl std::future::Future<Output = ()>) -> Result<()> {
  let jobber = prepare_jobs(deps).await;

  axum::Server::bind(deps.cfg.get_listen_addr())
    .serve(create_app(deps).await?.into_make_service())
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
  use crate::authentication::Claims;
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
  #[tokio::test(flavor = "multi_thread")]
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

    let mut args = Args::default();
    args.data_dir = ":memory:".into();

    let deps = Dependencies::make(args, Config::from_reader(&mut values).unwrap())
      .await
      .unwrap();

    let jobber = prepare_jobs(&deps).await;

    jobber.shutdown().await;
  }

  /// It makes sure the server can bind the address.
  #[tokio::test(flavor = "multi_thread")]
  async fn test_run_server() {
    let values = concat!(
      "listen_address: 127.0.0.1:0\n",
      "client:\n",
      "  uri: \"127.0.0.1:10000\"\n",
      "  token: \"rpcadmin\"\n",
      "jobs:\n",
      "  accounts_refresher: \"* * * * * * 1970\"\n",
      "  price_refresher: \"* * * * * * 1970\"\n",
      "pairs:\n",
      "  - [\"BTC\", \"USD\"]\n"
    );

    let mut values = values.as_bytes();

    let cfg = Config::from_reader(&mut values).unwrap();

    let mut args = Args::default();
    args.data_dir = ":memory:".to_string();

    let deps = Dependencies::make(args, cfg).await.unwrap();

    run_server(&deps, async {}).await.unwrap();
  }

  #[tokio::test(flavor = "multi_thread")]
  async fn test_get_account() {
    let mut secret_file = env::temp_dir();
    secret_file.push("secret.txt");

    let mut file = File::create(secret_file.clone()).unwrap();
    file.write_all(b"IUBePnVgKXFPc2QzZTRuSykuQic5IUt8QlY=").unwrap();

    let cfg = Config::default();

    let token =
      authentication::generate_token(&Claims::default(), &cfg.get_encoding_key(secret_file.to_str()).unwrap()).unwrap();

    let mut args = Args::default();
    args.secret_file = secret_file.to_str().map(String::from);
    args.data_dir = ":memory:".into();

    let deps = Dependencies::make(args, cfg).await.unwrap();

    let app = create_app(&deps).await.unwrap();

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
