use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::header::AUTHORIZATION,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde_json::{Value, json};
use tracing::warn;

use crate::{
    models::{ApiResponse, BatchRequest, DiscordUser, DiscordUserApi, Presence},
    socket,
    state::AppState,
};

pub(crate) fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/health/", get(health))
        .route("/v1/users/:id", get(get_user))
        .route("/v1/users/:id/", get(get_user))
        .route("/v1/users", post(get_users))
        .route("/v1/users/", post(get_users))
        .route("/socket", get(socket::handler))
        .route("/socket/", get(socket::handler))
        .fallback(not_found)
        .with_state(state)
}

async fn root() -> Json<Value> {
    Json(json!({
        "service": "neurobox-api",
        "routes": {
            "health": "/health",
            "user": "/v1/users/{discord_user_id}",
            "batch": "POST /v1/users",
            "socket": "/socket"
        }
    }))
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "cached_users": state.presences.read().await.len()
    }))
}

async fn not_found() -> Json<Value> {
    Json(json!({
        "success": false,
        "error": "route_not_found",
        "routes": {
            "index": "/",
            "health": "/health",
            "user": "/v1/users/{user_id}",
            "batch": "POST /v1/users",
            "socket": "/socket"
        }
    }))
}

async fn get_user(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    let presences = state.presences.read().await;
    if let Some(mut presence) = presences.get(&id).cloned() {
        drop(presences);
        if presence.discord_user.needs_profile() {
            if let Ok(profile) = get_or_fetch_user_profile(&state, &id).await {
                presence.discord_user = presence.discord_user.merge(profile);
            }
        }
        state.attach_kv(&mut presence).await;

        return Json(ApiResponse {
            success: true,
            data: presence,
        })
        .into_response();
    }
    drop(presences);

    match get_or_fetch_user_profile(&state, &id).await {
        Ok(discord_user) => {
            let mut presence = Presence::offline(discord_user);
            state.attach_kv(&mut presence).await;
            Json(ApiResponse {
                success: true,
                data: presence,
            })
            .into_response()
        }
        Err(error) => {
            warn!(%error, user_id = %id, "failed to fetch uncached discord user");
            Json(json!({
                "success": false,
                "error": "user_not_found_or_not_cached"
            }))
            .into_response()
        }
    }
}

async fn get_users(State(state): State<AppState>, Json(body): Json<BatchRequest>) -> Json<Value> {
    let requested: HashSet<_> = body.user_ids.into_iter().collect();
    let presences = state.presences.read().await;
    let kv = state.kv.read().await;
    let data: HashMap<_, _> = requested
        .iter()
        .filter_map(|id| {
            presences.get(id).cloned().map(|mut presence| {
                presence.kv = kv.get(id).cloned().unwrap_or_default();
                (id.clone(), presence)
            })
        })
        .collect();

    Json(json!({
        "success": true,
        "data": data
    }))
}

async fn get_or_fetch_user_profile(state: &AppState, id: &str) -> Result<DiscordUser> {
    if let Some(user) = state.users.read().await.get(id).cloned() {
        return Ok(user);
    }

    let user = fetch_user_profile(&state.token, id).await?;
    state
        .users
        .write()
        .await
        .insert(id.to_owned(), user.clone());
    Ok(user)
}

async fn fetch_user_profile(token: &str, id: &str) -> Result<DiscordUser> {
    let response = reqwest::Client::new()
        .get(format!("https://discord.com/api/v10/users/{id}"))
        .header(AUTHORIZATION, format!("Bot {token}"))
        .send()
        .await
        .context("discord user lookup failed")?;

    if !response.status().is_success() {
        anyhow::bail!("discord user lookup returned {}", response.status());
    }

    response
        .json::<DiscordUserApi>()
        .await
        .context("discord user payload was invalid")
        .map(DiscordUser::from_api)
}
