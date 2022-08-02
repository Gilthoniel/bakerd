use axum::{extract::Extension, routing::get, Router};
use std::net::SocketAddr;

use crate::repository::{account::SqliteAccountRepository, AsyncPool};

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate diesel_migrations;

mod controller;
mod model;
mod repository;
mod schema;

#[tokio::main]
async fn main() {
    let pool = AsyncPool::new("data.db");

    pool.run_migrations().await.unwrap();

    let repo = SqliteAccountRepository::new(pool);

    let app = Router::new()
        .route("/", get(controller::status))
        .route("/accounts/:addr", get(controller::get_account))
        .layer(Extension(repo));

    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
