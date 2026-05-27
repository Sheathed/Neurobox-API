use std::collections::{HashMap, HashSet};

use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::broadcast;
use tracing::{debug, warn};

use crate::state::AppState;

#[derive(Debug, Deserialize)]
struct SocketSubscribe {
    op: Option<u8>,
    d: Option<SocketSubscribeData>,
}

#[derive(Debug, Deserialize)]
struct SocketSubscribeData {
    user_id: Option<String>,
    user_ids: Option<Vec<String>>,
    all: Option<bool>,
    subscribe_to_id: Option<String>,
    subscribe_to_ids: Option<Vec<String>>,
    subscribe_to_all: Option<bool>,
}

enum Subscription {
    One(String),
    Many(HashSet<String>),
    All,
}

pub(crate) async fn handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(mut socket: WebSocket, state: AppState) {
    let mut subscription: Option<Subscription> = None;
    let mut events = state.events.subscribe();
    let mut seq = 0_u64;

    if socket
        .send(Message::Text(
            json!({
                "op": 1,
                "t": "HELLO",
                "d": {
                    "heartbeat_interval": 30_000
                }
            })
            .to_string(),
        ))
        .await
        .is_err()
    {
        return;
    }

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(message) = serde_json::from_str::<SocketSubscribe>(&text) {
                            if message.op == Some(2) {
                                subscription = message.d.and_then(Subscription::from_payload);
                                seq += 1;
                                if send_init_state(&mut socket, &state, subscription.as_ref(), seq).await.is_err() {
                                    return;
                                }
                            } else if message.op == Some(3) {
                                continue;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            return;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => return,
                    Some(Ok(_)) => {}
                    Some(Err(error)) => {
                        debug!(%error, "websocket receive error");
                        return;
                    }
                }
            }
            event = events.recv() => {
                match event {
                    Ok(event) => {
                        if !subscription_matches(subscription.as_ref(), &event.user_id) {
                            continue;
                        }
                        seq += 1;

                        let payload = json!({
                            "op": 0,
                            "seq": seq,
                            "t": "PRESENCE_UPDATE",
                            "d": event.presence
                        });

                        if socket.send(Message::Text(payload.to_string())).await.is_err() {
                            return;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        warn!(skipped, "websocket client lagged behind presence stream");
                    }
                    Err(broadcast::error::RecvError::Closed) => return,
                }
            }
        }
    }
}

async fn send_init_state(
    socket: &mut WebSocket,
    state: &AppState,
    subscription: Option<&Subscription>,
    seq: u64,
) -> Result<(), axum::Error> {
    let presences = state.presences.read().await;
    let data = match subscription {
        Some(Subscription::One(id)) => presences
            .get(id)
            .cloned()
            .and_then(|presence| serde_json::to_value(presence).ok())
            .unwrap_or(Value::Null),
        Some(Subscription::Many(ids)) => {
            let data: HashMap<_, _> = ids
                .iter()
                .filter_map(|id| {
                    presences
                        .get(id)
                        .cloned()
                        .map(|presence| (id.clone(), presence))
                })
                .collect();
            json!(data)
        }
        Some(Subscription::All) => {
            let data: HashMap<_, _> = presences
                .iter()
                .map(|(id, presence)| (id.clone(), presence.clone()))
                .collect();
            json!(data)
        }
        None => Value::Null,
    };

    socket
        .send(Message::Text(
            json!({
                "op": 0,
                "seq": seq,
                "t": "INIT_STATE",
                "d": data
            })
            .to_string(),
        ))
        .await
}

impl Subscription {
    fn from_payload(data: SocketSubscribeData) -> Option<Self> {
        if data.all.or(data.subscribe_to_all).unwrap_or(false) {
            return Some(Self::All);
        }

        if let Some(id) = data.user_id.or(data.subscribe_to_id) {
            return Some(Self::One(id));
        }

        data.user_ids
            .or(data.subscribe_to_ids)
            .map(|ids| Self::Many(ids.into_iter().collect()))
    }
}

fn subscription_matches(subscription: Option<&Subscription>, user_id: &str) -> bool {
    match subscription {
        Some(Subscription::One(id)) => id == user_id,
        Some(Subscription::Many(ids)) => ids.contains(user_id),
        Some(Subscription::All) => true,
        None => false,
    }
}
