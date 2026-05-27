use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use anyhow::Result;
use tokio::sync::{RwLock, broadcast};

use crate::{
    models::{DiscordUser, Presence, PresenceEvent},
    storage,
};

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) presences: Arc<RwLock<HashMap<String, Presence>>>,
    pub(crate) users: Arc<RwLock<HashMap<String, DiscordUser>>>,
    pub(crate) kv: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
    pub(crate) kv_path: PathBuf,
    pub(crate) commands_registered: Arc<AtomicBool>,
    pub(crate) events: broadcast::Sender<PresenceEvent>,
    pub(crate) token: String,
}

impl AppState {
    pub(crate) async fn new(token: String, kv_path: PathBuf) -> Result<Self> {
        let (events, _) = broadcast::channel(1024);
        let kv = storage::load_kv(&kv_path).await?;

        Ok(Self {
            presences: Arc::new(RwLock::new(HashMap::new())),
            users: Arc::new(RwLock::new(HashMap::new())),
            kv: Arc::new(RwLock::new(kv)),
            kv_path,
            commands_registered: Arc::new(AtomicBool::new(false)),
            events,
            token,
        })
    }

    pub(crate) async fn user_kv(&self, user_id: &str) -> HashMap<String, String> {
        self.kv
            .read()
            .await
            .get(user_id)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) async fn set_kv(&self, user_id: &str, key: String, value: String) -> Result<()> {
        let snapshot = {
            let mut kv = self.kv.write().await;
            kv.entry(user_id.to_owned()).or_default().insert(key, value);
            kv.clone()
        };

        storage::save_kv(&self.kv_path, &snapshot).await?;
        self.publish_presence_update(user_id).await;
        Ok(())
    }

    pub(crate) async fn delete_kv(&self, user_id: &str, key: &str) -> Result<bool> {
        let (removed, snapshot) = {
            let mut kv = self.kv.write().await;
            let removed = kv
                .get_mut(user_id)
                .and_then(|values| values.remove(key))
                .is_some();

            if kv.get(user_id).is_some_and(HashMap::is_empty) {
                kv.remove(user_id);
            }

            (removed, kv.clone())
        };

        if removed {
            storage::save_kv(&self.kv_path, &snapshot).await?;
            self.publish_presence_update(user_id).await;
        }

        Ok(removed)
    }

    pub(crate) async fn attach_kv(&self, presence: &mut Presence) {
        presence.kv = self.user_kv(&presence.user_id).await;
    }

    pub(crate) fn claim_command_registration(&self) -> bool {
        self.commands_registered
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }

    async fn publish_presence_update(&self, user_id: &str) {
        let Some(mut presence) = self.presences.read().await.get(user_id).cloned() else {
            return;
        };

        self.attach_kv(&mut presence).await;
        self.presences
            .write()
            .await
            .insert(user_id.to_owned(), presence.clone());

        let _ = self.events.send(PresenceEvent {
            user_id: user_id.to_owned(),
            presence,
        });
    }
}
