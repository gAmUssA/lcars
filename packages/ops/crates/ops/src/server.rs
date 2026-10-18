//! Plugin-facing server on loopback: `GET /sessions` for a snapshot,
//! `GET /ws` for a push stream, `GET /healthz`.
//!
//! Every WebSocket message is the full `FleetView`; the plugin repaints
//! from it. Deltas can come later if the payload ever gets big.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing::{debug, info};

use crate::model::{Fleet, HostStatus, Session};

/// What the plugin receives: hosts plus sessions in display order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetView {
    pub hosts: Vec<HostStatus>,
    pub sessions: Vec<Session>,
}

impl FleetView {
    pub fn from_fleet(fleet: &Fleet) -> Self {
        FleetView {
            hosts: fleet.hosts.values().cloned().collect(),
            sessions: fleet.sessions_sorted().into_iter().cloned().collect(),
        }
    }
}

pub type FleetRx = watch::Receiver<Arc<FleetView>>;

#[derive(Clone)]
struct AppState {
    fleet: FleetRx,
}

pub fn router(fleet: FleetRx) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/sessions", get(sessions))
        .route("/ws", get(ws_upgrade))
        .with_state(AppState { fleet })
}

/// Serve until the listener fails. Caller owns binding so tests can use port 0.
pub async fn serve(listener: TcpListener, fleet: FleetRx) -> std::io::Result<()> {
    info!(addr = %listener.local_addr()?, "serving plugin API");
    axum::serve(listener, router(fleet)).await
}

async fn sessions(State(state): State<AppState>) -> Json<FleetView> {
    let view: FleetView = (**state.fleet.borrow()).clone();
    Json(view)
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_stream(socket, state.fleet))
}

async fn ws_stream(mut socket: WebSocket, mut fleet: FleetRx) {
    debug!("plugin websocket connected");
    // Send the current view immediately, then one message per change.
    let initial = fleet.borrow_and_update().clone();
    if send_view(&mut socket, &initial).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            changed = fleet.changed() => {
                if changed.is_err() {
                    break; // publisher gone: shutting down
                }
                let view = fleet.borrow_and_update().clone();
                if send_view(&mut socket, &view).await.is_err() {
                    break;
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                    Some(Ok(_)) => {} // the plugin sends nothing we act on yet
                }
            }
        }
    }
    debug!("plugin websocket closed");
}

async fn send_view(socket: &mut WebSocket, view: &FleetView) -> Result<(), axum::Error> {
    let text = serde_json::to_string(view).expect("FleetView serialises");
    socket.send(Message::Text(text.into())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AgentState, HostEvent};
    use futures_util::StreamExt;
    use herdr_compat::api::schema::PaneAgentStatusChangedEvent;
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn fleet_with_one_blocked_session() -> Fleet {
        let mut fleet = Fleet::with_hosts(["ds9"], 1_000);
        fleet.apply(
            "ds9",
            HostEvent::Connected {
                version: "0.9.0".into(),
                protocol: 22,
            },
            1_001,
        );
        fleet.apply(
            "ds9",
            HostEvent::StatusChanged(PaneAgentStatusChangedEvent {
                pane_id: "w7:p1".into(),
                workspace_id: "w7".into(),
                agent_status: herdr_compat::api::schema::AgentStatus::Blocked,
                agent: Some("claude".into()),
                title: Some("approve?".into()),
                display_agent: None,
                state_labels: HashMap::new(),
            }),
            1_002,
        );
        fleet
    }

    async fn start() -> (std::net::SocketAddr, watch::Sender<Arc<FleetView>>) {
        let fleet = fleet_with_one_blocked_session();
        let (tx, rx) = watch::channel(Arc::new(FleetView::from_fleet(&fleet)));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener, rx));
        (addr, tx)
    }

    #[tokio::test]
    async fn get_sessions_returns_hosts_and_sorted_sessions_as_json() {
        let (addr, _tx) = start().await;
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                format!("GET /sessions HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).await.unwrap();
        let text = String::from_utf8(raw).unwrap();
        assert!(text.starts_with("HTTP/1.1 200"), "{text}");
        assert!(text.contains("content-type: application/json"), "{text}");
        let body = text.split("\r\n\r\n").nth(1).unwrap();
        let view: FleetView = serde_json::from_str(body).unwrap();
        assert_eq!(view.hosts.len(), 1);
        assert!(view.hosts[0].connected);
        assert_eq!(view.sessions.len(), 1);
        assert_eq!(view.sessions[0].id, "ds9/w7:p1");
        assert_eq!(view.sessions[0].state, AgentState::Blocked);
        let json: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(json["sessions"][0]["state"], "blocked");
    }

    #[tokio::test]
    async fn healthz_is_ok() {
        let (addr, _tx) = start().await;
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                format!("GET /healthz HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n\r\n")
                    .as_bytes(),
            )
            .await
            .unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).await.unwrap();
        assert!(raw.starts_with("HTTP/1.1 200"), "{raw}");
        assert!(raw.ends_with("ok"), "{raw}");
    }

    #[tokio::test]
    async fn websocket_sends_current_view_then_each_change() {
        let (addr, tx) = start().await;
        let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
            .await
            .unwrap();
        let first = ws.next().await.unwrap().unwrap().into_text().unwrap();
        let view: FleetView = serde_json::from_str(&first).unwrap();
        assert_eq!(view.sessions[0].state, AgentState::Blocked);

        let mut fleet = fleet_with_one_blocked_session();
        fleet.apply(
            "ds9",
            HostEvent::PaneClosed {
                pane_id: "w7:p1".into(),
            },
            1_003,
        );
        tx.send(Arc::new(FleetView::from_fleet(&fleet))).unwrap();
        let second = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
            .into_text()
            .unwrap();
        let view: FleetView = serde_json::from_str(&second).unwrap();
        assert!(view.sessions.is_empty());
        assert_eq!(view.hosts.len(), 1);
    }
}
