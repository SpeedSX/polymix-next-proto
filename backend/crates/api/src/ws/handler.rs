//! `GET /api/ws?token=<jwt>` — authenticates itself (wired outside the
//! `require_auth` layer): the token comes from the query string because
//! browsers can't set headers on WebSocket upgrades. Rejection happens
//! before the upgrade, as the protocol requires.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::response::Response;
use serde::Deserialize;
use tokio::sync::broadcast::error::RecvError;

use crate::auth::authenticate_token;
use crate::error::ApiError;
use crate::state::AppState;
use crate::ws::hub::Hub;

const PING_INTERVAL: Duration = Duration::from_secs(30);
const PING_FRAME: &str = r#"{"type":"ping"}"#;
const RESYNC_FRAME: &str = r#"{"type":"resync"}"#;

#[derive(Deserialize)]
pub struct WsQuery {
    token: Option<String>,
}

pub async fn ws(
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    upgrade: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let token = query
        .token
        .ok_or_else(|| ApiError::unauthorized("missing token"))?;
    let (auth, _tenant) = authenticate_token(&state, &token).await?;

    Ok(upgrade.on_upgrade(move |socket| handle_socket(state, auth.tenant_db, socket)))
}

/// Releases the hub slot on any exit — normal close, send error, or panic —
/// so the subscriber count can't leak.
struct SubscriptionGuard {
    hub: Arc<Hub>,
    tenant_db: String,
}

impl Drop for SubscriptionGuard {
    fn drop(&mut self) {
        let hub = self.hub.clone();
        let tenant_db = std::mem::take(&mut self.tenant_db);
        tokio::spawn(async move { hub.unsubscribe(&tenant_db).await });
    }
}

async fn handle_socket(state: AppState, tenant_db: String, mut socket: WebSocket) {
    let mut rx = state.hub.subscribe(&tenant_db).await;
    let _guard = SubscriptionGuard {
        hub: state.hub.clone(),
        tenant_db,
    };

    let mut ping = tokio::time::interval(PING_INTERVAL);
    // An interval's first tick completes immediately; the first ping should
    // come a full period after connect.
    ping.tick().await;

    loop {
        tokio::select! {
            event = rx.recv() => {
                let frame = match event {
                    Ok(event) => match serde_json::to_string(event.as_ref()) {
                        Ok(json) => json,
                        Err(err) => {
                            tracing::error!(error = %err, "failed to serialize server event");
                            continue;
                        }
                    },
                    // This subscriber fell behind the broadcast buffer;
                    // events were dropped, so tell the client to refetch
                    // instead of dropping the connection.
                    Err(RecvError::Lagged(_)) => RESYNC_FRAME.to_string(),
                    Err(RecvError::Closed) => break,
                };
                if socket.send(Message::Text(frame.into())).await.is_err() {
                    break;
                }
            }
            _ = ping.tick() => {
                if socket.send(Message::Text(PING_FRAME.into())).await.is_err() {
                    break;
                }
            }
            msg = socket.recv() => {
                match msg {
                    // The only expected client message is `{"type":"pong"}`;
                    // anything else non-close is ignored per the protocol.
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    Some(Ok(_)) => {}
                }
            }
        }
    }
}
