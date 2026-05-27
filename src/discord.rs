use std::{env, time::Duration};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use reqwest::header::AUTHORIZATION;
use serde_json::{Value, json};
use tokio::{net::TcpStream, time};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest, http::header::USER_AGENT},
};
use tracing::{debug, error, warn};

use crate::{
    config::Config,
    models::{
        Activity, ActivityAssets, ActivityTimestamps, ClientStatus, DiscordUser, Presence,
        PresenceEvent, Spotify, avatar_url,
    },
    state::AppState,
};

const DISCORD_GATEWAY_URL: &str = "wss://gateway.discord.gg/?v=10&encoding=json";
const INTENT_GUILDS: u64 = 1 << 0;
const INTENT_GUILD_MEMBERS: u64 = 1 << 1;
const INTENT_GUILD_PRESENCES: u64 = 1 << 8;

type DiscordWrite =
    futures_util::stream::SplitSink<WebSocketStream<MaybeTlsStream<TcpStream>>, Message>;

pub(crate) async fn gateway_loop(config: Config, state: AppState) {
    let mut reconnect_delay = Duration::from_secs(2);

    loop {
        match run_gateway(&config, &state).await {
            Ok(()) => warn!("discord gateway session ended"),
            Err(error) => error!(%error, "discord gateway error"),
        }

        time::sleep(reconnect_delay).await;
        reconnect_delay = (reconnect_delay * 2).min(Duration::from_secs(60));
    }
}

async fn run_gateway(config: &Config, state: &AppState) -> Result<()> {
    let mut request = DISCORD_GATEWAY_URL.into_client_request()?;
    request
        .headers_mut()
        .insert(USER_AGENT, "Neurobox-API (https://yaw.cx, 0.1)".parse()?);

    let (socket, _) = connect_async(request)
        .await
        .context("gateway connect failed")?;
    let (mut write, mut read) = socket.split();

    while let Some(message) = read.next().await {
        let text = match message? {
            Message::Text(text) => text,
            Message::Close(_) => break,
            _ => continue,
        };
        let payload: Value = serde_json::from_str(&text)?;

        match payload["op"].as_u64() {
            Some(10) => {
                let interval_ms = payload["d"]["heartbeat_interval"]
                    .as_u64()
                    .unwrap_or(45_000);
                identify(&mut write, &config.token).await?;
                start_heartbeat(write, Duration::from_millis(interval_ms));
                break;
            }
            _ => debug!(?payload, "received gateway message before hello"),
        }
    }

    while let Some(message) = read.next().await {
        let text = match message? {
            Message::Text(text) => text,
            Message::Close(_) => break,
            _ => continue,
        };
        let payload: Value = serde_json::from_str(&text)?;

        match payload["op"].as_u64() {
            Some(0) => dispatch_event(config, state, &payload).await,
            Some(7) => anyhow::bail!("discord requested reconnect"),
            Some(9) => anyhow::bail!("discord invalidated the gateway session"),
            Some(11) => debug!("discord heartbeat acknowledged"),
            _ => {}
        }
    }

    Ok(())
}

fn start_heartbeat(mut write: DiscordWrite, interval: Duration) {
    tokio::spawn(async move {
        let mut ticker = time::interval(interval);
        loop {
            ticker.tick().await;
            if write
                .send(Message::Text(json!({ "op": 1, "d": null }).to_string()))
                .await
                .is_err()
            {
                break;
            }
        }
    });
}

async fn identify(write: &mut DiscordWrite, token: &str) -> Result<()> {
    let payload = json!({
        "op": 2,
        "d": {
            "token": token,
            "intents": INTENT_GUILDS | INTENT_GUILD_MEMBERS | INTENT_GUILD_PRESENCES,
            "properties": {
                "os": env::consts::OS,
                "browser": "neurobox-api",
                "device": "neurobox-api"
            }
        }
    });

    write.send(Message::Text(payload.to_string())).await?;
    Ok(())
}

async fn dispatch_event(config: &Config, state: &AppState, payload: &Value) {
    match payload["t"].as_str() {
        Some("READY") => {
            if state.claim_command_registration() {
                if let Some(application_id) = payload["d"]["application"]["id"].as_str() {
                    if let Err(error) = register_kv_command(config, application_id).await {
                        error!(%error, "failed to register Discord slash command");
                    }
                }
            }

            if let Some(presences) = payload["d"]["presences"].as_array() {
                for value in presences {
                    if let Some(presence) = parse_presence(value) {
                        cache_presence(state, presence).await;
                    }
                }
            }
        }
        Some("PRESENCE_UPDATE") => {
            if let Some(presence) = parse_presence(&payload["d"]) {
                cache_presence(state, presence).await;
            }
        }
        Some("GUILD_CREATE") => {
            if let Some(presences) = payload["d"]["presences"].as_array() {
                for value in presences {
                    if let Some(presence) = parse_presence(value) {
                        cache_presence(state, presence).await;
                    }
                }
            }
        }
        Some("INTERACTION_CREATE") => handle_interaction(&config.token, state, &payload["d"]).await,
        _ => {}
    }
}

async fn register_kv_command(config: &Config, application_id: &str) -> Result<()> {
    let command = json!({
        "name": "kv",
        "description": "Manage your Neurobox profile metadata",
        "options": [
            {
                "type": 1,
                "name": "set",
                "description": "Set one profile value",
                "options": [
                    {
                        "type": 3,
                        "name": "key",
                        "description": "The field name, like site or bio",
                        "required": true
                    },
                    {
                        "type": 3,
                        "name": "value",
                        "description": "The value to save",
                        "required": true
                    }
                ]
            },
            {
                "type": 1,
                "name": "delete",
                "description": "Delete one profile value",
                "options": [
                    {
                        "type": 3,
                        "name": "key",
                        "description": "The field name to delete",
                        "required": true
                    }
                ]
            },
            {
                "type": 1,
                "name": "list",
                "description": "Show your saved profile values"
            }
        ]
    });

    let url = match &config.guild_id {
        Some(guild_id) => {
            format!(
                "https://discord.com/api/v10/applications/{application_id}/guilds/{guild_id}/commands"
            )
        }
        None => format!("https://discord.com/api/v10/applications/{application_id}/commands"),
    };

    let response = reqwest::Client::new()
        .post(url)
        .header(AUTHORIZATION, format!("Bot {}", config.token))
        .json(&command)
        .send()
        .await
        .context("Discord command registration failed")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Discord command registration returned {}",
            response.status()
        );
    }

    Ok(())
}

async fn handle_interaction(token: &str, state: &AppState, interaction: &Value) {
    if interaction["type"].as_u64() != Some(2) || interaction["data"]["name"].as_str() != Some("kv")
    {
        return;
    }

    let interaction_id = match interaction["id"].as_str() {
        Some(id) => id,
        None => return,
    };
    let interaction_token = match interaction["token"].as_str() {
        Some(token) => token,
        None => return,
    };
    let user_id = match interaction["member"]["user"]["id"]
        .as_str()
        .or_else(|| interaction["user"]["id"].as_str())
    {
        Some(id) => id.to_owned(),
        None => return,
    };

    let response = match interaction["data"]["options"]
        .as_array()
        .and_then(|options| options.first())
    {
        Some(command) if command["name"].as_str() == Some("set") => {
            let key = option_value(command, "key");
            let value = option_value(command, "value");

            match (key, value) {
                (Some(key), Some(value)) => {
                    match state.set_kv(&user_id, key.clone(), value.clone()).await {
                        Ok(()) => format!("Saved `{key}`."),
                        Err(error) => {
                            error!(%error, user_id, "failed to save KV from Discord command");
                            "I couldn't save that value.".to_owned()
                        }
                    }
                }
                _ => "Use `/kv set key:<name> value:<value>`.".to_owned(),
            }
        }
        Some(command) if command["name"].as_str() == Some("delete") => {
            match option_value(command, "key") {
                Some(key) => match state.delete_kv(&user_id, &key).await {
                    Ok(true) => format!("Deleted `{key}`."),
                    Ok(false) => format!("`{key}` was not set."),
                    Err(error) => {
                        error!(%error, user_id, "failed to delete KV from Discord command");
                        "I couldn't delete that value.".to_owned()
                    }
                },
                None => "Use `/kv delete key:<name>`.".to_owned(),
            }
        }
        Some(command) if command["name"].as_str() == Some("list") => {
            let kv = state.user_kv(&user_id).await;
            if kv.is_empty() {
                "You do not have any saved values yet.".to_owned()
            } else {
                let mut items: Vec<_> = kv.into_iter().collect();
                items.sort_by(|a, b| a.0.cmp(&b.0));
                items
                    .into_iter()
                    .map(|(key, value)| format!("`{key}` = {value}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        _ => "Use `/kv set`, `/kv delete`, or `/kv list`.".to_owned(),
    };

    if let Err(error) =
        respond_to_interaction(token, interaction_id, interaction_token, &response).await
    {
        error!(%error, "failed to respond to Discord interaction");
    }
}

fn option_value(command: &Value, name: &str) -> Option<String> {
    command["options"]
        .as_array()?
        .iter()
        .find(|option| option["name"].as_str() == Some(name))?
        .get("value")?
        .as_str()
        .map(ToOwned::to_owned)
}

async fn respond_to_interaction(
    bot_token: &str,
    interaction_id: &str,
    interaction_token: &str,
    content: &str,
) -> Result<()> {
    let response = reqwest::Client::new()
        .post(format!(
            "https://discord.com/api/v10/interactions/{interaction_id}/{interaction_token}/callback"
        ))
        .header(AUTHORIZATION, format!("Bot {bot_token}"))
        .json(&json!({
            "type": 4,
            "data": {
                "content": content,
                "flags": 64
            }
        }))
        .send()
        .await
        .context("Discord interaction callback failed")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "Discord interaction callback returned {}",
            response.status()
        );
    }

    Ok(())
}

async fn cache_presence(state: &AppState, mut presence: Presence) {
    if !presence.discord_user.needs_profile() {
        state
            .users
            .write()
            .await
            .insert(presence.user_id.clone(), presence.discord_user.clone());
    }

    presence.kv = state
        .kv
        .read()
        .await
        .get(&presence.user_id)
        .cloned()
        .unwrap_or_default();

    state
        .presences
        .write()
        .await
        .insert(presence.user_id.clone(), presence.clone());

    let _ = state.events.send(PresenceEvent {
        user_id: presence.user_id.clone(),
        presence,
    });
}

fn parse_presence(value: &Value) -> Option<Presence> {
    let discord_user = parse_discord_user(&value["user"])?;
    let user_id = discord_user.id.clone();
    let discord_status = value["status"].as_str().unwrap_or("offline").to_owned();
    let client_status: ClientStatus =
        serde_json::from_value(value["client_status"].clone()).unwrap_or_default();
    let activities: Vec<Activity> = value["activities"]
        .as_array()
        .map(|items| items.iter().filter_map(parse_activity).collect())
        .unwrap_or_default();
    let spotify = activities.iter().find_map(parse_spotify);

    Some(Presence {
        user_id,
        discord_user,
        discord_status,
        active_on_discord_desktop: client_status.desktop.is_some(),
        active_on_discord_mobile: client_status.mobile.is_some(),
        active_on_discord_web: client_status.web.is_some(),
        listening_to_spotify: spotify.is_some(),
        spotify,
        activities,
        kv: Default::default(),
    })
}

fn parse_discord_user(value: &Value) -> Option<DiscordUser> {
    let id = value["id"].as_str()?.to_owned();
    Some(DiscordUser {
        id: id.clone(),
        username: optional_string(&value["username"]),
        discriminator: optional_string(&value["discriminator"]),
        avatar: optional_string(&value["avatar"]).map(|avatar| avatar_url(&id, &avatar)),
        display_name: optional_string(&value["global_name"]),
    })
}

fn parse_activity(value: &Value) -> Option<Activity> {
    Some(Activity {
        name: value["name"].as_str()?.to_owned(),
        kind: value["type"].as_u64()? as u8,
        state: optional_string(&value["state"]),
        details: optional_string(&value["details"]),
        application_id: optional_string(&value["application_id"]),
        sync_id: optional_string(&value["sync_id"]),
        timestamps: serde_json::from_value::<ActivityTimestamps>(value["timestamps"].clone()).ok(),
        assets: serde_json::from_value::<ActivityAssets>(value["assets"].clone()).ok(),
        emoji: optional_value(&value["emoji"]),
        party: optional_value(&value["party"]),
        flags: value["flags"].as_u64(),
        buttons: optional_value(&value["buttons"]),
    })
}

fn parse_spotify(activity: &Activity) -> Option<Spotify> {
    if activity.kind != 2 || activity.name != "Spotify" {
        return None;
    }

    Some(Spotify {
        track_id: activity.sync_id(),
        timestamps: activity.timestamps.clone(),
        album: activity
            .assets
            .as_ref()
            .and_then(|assets| assets.large_text.clone()),
        album_art_url: activity
            .assets
            .as_ref()
            .and_then(|assets| assets.large_image.as_deref())
            .and_then(spotify_album_art_url),
        artist: activity.state.clone(),
        song: activity.details.clone(),
    })
}

fn optional_string(value: &Value) -> Option<String> {
    value.as_str().map(ToOwned::to_owned)
}

fn optional_value(value: &Value) -> Option<Value> {
    (!value.is_null()).then(|| value.clone())
}

fn spotify_album_art_url(asset: &str) -> Option<String> {
    asset
        .strip_prefix("spotify:")
        .map(|image| format!("https://i.scdn.co/image/{image}"))
}
