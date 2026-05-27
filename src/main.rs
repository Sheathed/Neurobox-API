mod config;
mod discord;
mod models;
mod routes;
mod socket;
mod state;
mod storage;

use std::env;

use anyhow::{Context, Result};
use tokio::net::TcpListener;
use tracing::info;

use crate::{config::Config, routes::router, state::AppState};

#[tokio::main]
async fn main() -> Result<()> {
    dotenvy::dotenv().ok();
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_env_filter(
            env::var("RUST_LOG").unwrap_or_else(|_| "neurobox_api=info,tower_http=info".into()),
        )
        .init();

    let config = Config::from_env()?;
    let state = AppState::new(config.token.clone(), config.kv_path.clone()).await?;

    tokio::spawn(discord::gateway_loop(config.clone(), state.clone()));

    let listener = TcpListener::bind(config.bind)
        .await
        .with_context(|| format!("failed to bind {}", config.bind))?;

    info!("listening on http://{}", config.bind);
    axum::serve(listener, router(state)).await?;
    Ok(())
}
