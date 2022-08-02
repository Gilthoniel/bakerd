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

diesel_migrations::embed_migrations!();

#[tokio::main]
async fn main() {
    let pool = AsyncPool::new("");

    embedded_migrations::run(&pool.get_conn().await.unwrap()).expect("unable to run migrations");

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
