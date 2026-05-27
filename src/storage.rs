use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result};
use tokio::fs;

pub(crate) type KvStore = HashMap<String, HashMap<String, String>>;

pub(crate) async fn load_kv(path: &Path) -> Result<KvStore> {
    match fs::read_to_string(path).await {
        Ok(contents) => serde_json::from_str(&contents).context("failed to parse KV storage"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(HashMap::new()),
        Err(error) => Err(error).context("failed to read KV storage"),
    }
}

pub(crate) async fn save_kv(path: &Path, kv: &KvStore) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .context("failed to create KV storage directory")?;
    }

    let contents = serde_json::to_string_pretty(kv).context("failed to serialize KV storage")?;
    fs::write(path, contents)
        .await
        .context("failed to write KV storage")
}
