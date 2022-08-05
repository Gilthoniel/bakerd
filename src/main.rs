use axum::{extract::Extension, routing::get, Router};
use env_logger::Env;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::signal;

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
async fn main() {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    let cfg = parse_config("./config.yaml");

    let pool = AsyncPool::new("data.db");

    pool.run_migrations().await.unwrap();

    let repo = SqliteAccountRepository::new(pool);

    // Schedule the jobs
    let mut scheduler = job::Scheduler::new();

    for (name, schedule_str) in cfg.get_jobs() {
        let schedule = cron::Schedule::from_str(schedule_str).unwrap();

        let job = match name.as_str() {
            "refresh-accounts" => Box::new(job::account::RefreshAccountsJob::new()),
            _ => panic!("job [{}] is unknown", name),
        };

        scheduler.register(name, schedule, job);
    }

    let jobber = scheduler.start();

    let app = Router::new()
        .route("/", get(controller::status))
        .route("/accounts/:addr", get(controller::get_account))
        .layer(Extension(repo));

    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    // Shutdown the job controller.
    jobber.shutdown().await;
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.unwrap();
    };

    tokio::select! {
        _ = ctrl_c => {},
    }
}

fn parse_config(filepath: &str) -> config::Config {
    let file = std::fs::File::open(filepath).expect("unable to open config file");

    serde_yaml::from_reader(file).expect("failed to deserialize the config")
}
