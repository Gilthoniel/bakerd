use axum::{extract::Extension, routing::get, Router};
use env_logger::Env;
use std::net::SocketAddr;
use tokio::signal;

use crate::repository::{account::SqliteAccountRepository, AsyncPool};

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate diesel_migrations;

#[macro_use]
extern crate async_trait;

mod controller;
mod job;
mod model;
mod repository;
mod schema;

#[tokio::main]
async fn main() {
    env_logger::init_from_env(Env::default().default_filter_or("info"));

    let pool = AsyncPool::new("data.db");

    pool.run_migrations().await.unwrap();

    let repo = SqliteAccountRepository::new(pool);

    // Schedule the jobs
    let scheduler = job::Scheduler::new();

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
