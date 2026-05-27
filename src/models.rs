use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Presence {
    pub(crate) user_id: String,
    pub(crate) discord_user: DiscordUser,
    pub(crate) discord_status: String,
    pub(crate) activities: Vec<Activity>,
    pub(crate) active_on_discord_desktop: bool,
    pub(crate) active_on_discord_mobile: bool,
    pub(crate) active_on_discord_web: bool,
    pub(crate) listening_to_spotify: bool,
    pub(crate) spotify: Option<Spotify>,
    pub(crate) kv: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DiscordUser {
    pub(crate) id: String,
    pub(crate) username: Option<String>,
    pub(crate) discriminator: Option<String>,
    pub(crate) avatar: Option<String>,
    pub(crate) display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct DiscordUserApi {
    pub(crate) id: String,
    pub(crate) username: Option<String>,
    pub(crate) discriminator: Option<String>,
    pub(crate) avatar: Option<String>,
    pub(crate) global_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Activity {
    pub(crate) name: String,
    #[serde(rename = "type")]
    pub(crate) kind: u8,
    pub(crate) state: Option<String>,
    pub(crate) details: Option<String>,
    pub(crate) application_id: Option<String>,
    pub(crate) sync_id: Option<String>,
    pub(crate) timestamps: Option<ActivityTimestamps>,
    pub(crate) assets: Option<ActivityAssets>,
    pub(crate) emoji: Option<Value>,
    pub(crate) party: Option<Value>,
    pub(crate) flags: Option<u64>,
    pub(crate) buttons: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ActivityTimestamps {
    pub(crate) start: Option<u64>,
    pub(crate) end: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ActivityAssets {
    pub(crate) large_image: Option<String>,
    pub(crate) large_text: Option<String>,
    pub(crate) small_image: Option<String>,
    pub(crate) small_text: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct ClientStatus {
    pub(crate) desktop: Option<String>,
    pub(crate) mobile: Option<String>,
    pub(crate) web: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Spotify {
    pub(crate) track_id: Option<String>,
    pub(crate) timestamps: Option<ActivityTimestamps>,
    pub(crate) album: Option<String>,
    pub(crate) album_art_url: Option<String>,
    pub(crate) artist: Option<String>,
    pub(crate) song: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PresenceEvent {
    pub(crate) user_id: String,
    pub(crate) presence: Presence,
}

#[derive(Debug, Serialize)]
pub(crate) struct ApiResponse<T> {
    pub(crate) success: bool,
    pub(crate) data: T,
}

#[derive(Debug, Deserialize)]
pub(crate) struct BatchRequest {
    pub(crate) user_ids: Vec<String>,
}

impl Presence {
    pub(crate) fn offline(discord_user: DiscordUser) -> Self {
        Self {
            user_id: discord_user.id.clone(),
            discord_user,
            discord_status: "offline".into(),
            active_on_discord_desktop: false,
            active_on_discord_mobile: false,
            active_on_discord_web: false,
            listening_to_spotify: false,
            spotify: None,
            activities: Vec::new(),
            kv: HashMap::new(),
        }
    }
}

impl DiscordUser {
    pub(crate) fn needs_profile(&self) -> bool {
        self.username.is_none() || self.avatar.is_none() || self.display_name.is_none()
    }

    pub(crate) fn merge(self, profile: DiscordUser) -> Self {
        Self {
            id: self.id,
            username: self.username.or(profile.username),
            discriminator: self.discriminator.or(profile.discriminator),
            avatar: self.avatar.or(profile.avatar),
            display_name: self.display_name.or(profile.display_name),
        }
    }

    pub(crate) fn from_api(value: DiscordUserApi) -> Self {
        Self {
            avatar: value
                .avatar
                .as_deref()
                .map(|avatar| avatar_url(&value.id, avatar)),
            display_name: value.global_name,
            id: value.id,
            username: value.username,
            discriminator: value.discriminator,
        }
    }
}

impl Activity {
    pub(crate) fn sync_id(&self) -> Option<String> {
        self.sync_id.clone()
    }
}

pub(crate) fn avatar_url(user_id: &str, avatar: &str) -> String {
    let ext = if avatar.starts_with("a_") {
        "gif"
    } else {
        "png"
    };
    format!("https://cdn.discordapp.com/avatars/{user_id}/{avatar}.{ext}?size=1024")
}
