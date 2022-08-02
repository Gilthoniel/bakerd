use axum::{routing::get, extract::Extension, Router};
use std::net::SocketAddr;
use std::sync::Arc;

use crate::repository::{DynRepository, SqliteRepository};

mod controller;
mod model;
mod repository;

#[tokio::main]
async fn main() {
    let repo = Arc::new(SqliteRepository) as DynRepository;

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


