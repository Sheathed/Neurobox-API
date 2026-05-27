use std::{env, net::SocketAddr, path::PathBuf};

use anyhow::{Context, Result};

#[derive(Debug, Clone)]
pub(crate) struct Config {
    pub(crate) token: String,
    pub(crate) bind: SocketAddr,
    pub(crate) kv_path: PathBuf,
    pub(crate) guild_id: Option<String>,
}

impl Config {
    pub(crate) fn from_env() -> Result<Self> {
        let token = env::var("DISCORD_TOKEN").context("DISCORD_TOKEN is required")?;
        let host = env::var("HOST").unwrap_or_else(|_| "127.0.0.1".into());
        let port = env::var("PORT")
            .unwrap_or_else(|_| "3000".into())
            .parse::<u16>()
            .context("PORT must be a valid u16")?;
        let kv_path = env::var("KV_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("storage/kv.json"));
        let guild_id = env::var("GUILD_ID")
            .ok()
            .filter(|value| !value.trim().is_empty());

        Ok(Self {
            token,
            bind: format!("{host}:{port}").parse()?,
            kv_path,
            guild_id,
        })
    }
}
