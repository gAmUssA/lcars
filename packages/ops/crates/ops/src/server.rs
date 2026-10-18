//! Plugin-facing server on loopback: `GET /sessions` for a snapshot,
//! `GET /ws` for a push stream, `GET /healthz`.
//!
//! Every WebSocket message is the full `FleetView`; the plugin repaints
//! from it. Deltas can come later if the payload ever gets big.

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tracing::{debug, info, warn};

use crate::actions::ActionRelay;
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
    relay: Arc<ActionRelay>,
}

pub fn router(fleet: FleetRx, relay: Arc<ActionRelay>) -> Router {
    Router::new()
        .route("/healthz", get(|| async { "ok" }))
        .route("/sessions", get(sessions))
        .route("/actions", post(actions))
        .route("/ws", get(ws_upgrade))
        .with_state(AppState { fleet, relay })
}

/// Serve until the listener fails. Caller owns binding so tests can use port 0.
pub async fn serve(
    listener: TcpListener,
    fleet: FleetRx,
    relay: Arc<ActionRelay>,
) -> std::io::Result<()> {
    info!(addr = %listener.local_addr()?, "serving plugin API");
    axum::serve(listener, router(fleet, relay)).await
}

/// `POST /actions` body. `session` is a `FleetView` session id (`host/pane`).
///
/// Actions: `focus`; `scroll` with `delta` rows (positive scrolls up into
/// scrollback); `prompt` with `text` submitted to the agent plus Enter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRequest {
    pub action: String,
    pub session: String,
    #[serde(default)]
    pub delta: Option<i64>,
    #[serde(default)]
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// `scroll`: the offset from bottom that was applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_from_bottom: Option<u64>,
}

fn action_error(status: StatusCode, error: String) -> (StatusCode, Json<ActionResponse>) {
    (
        status,
        Json(ActionResponse {
            ok: false,
            error: Some(error),
            offset_from_bottom: None,
        }),
    )
}

async fn actions(
    State(state): State<AppState>,
    Json(request): Json<ActionRequest>,
) -> (StatusCode, Json<ActionResponse>) {
    let session = {
        let view = state.fleet.borrow();
        view.sessions
            .iter()
            .find(|s| s.id == request.session)
            .cloned()
    };
    let Some(session) = session else {
        return action_error(
            StatusCode::NOT_FOUND,
            format!(
                "no session `{}` — the key may be stale, wait for the next update",
                request.session
            ),
        );
    };
    let result: anyhow::Result<Option<u64>> = match request.action.as_str() {
        "focus" => state.relay.focus(&session).await.map(|()| None),
        "scroll" => match request.delta {
            Some(delta) => state.relay.scroll(&session, delta).await.map(Some),
            None => {
                return action_error(
                    StatusCode::BAD_REQUEST,
                    "scroll needs `delta` (rows, positive = up)".to_string(),
                )
            }
        },
        "prompt" => match request.text.as_deref() {
            Some(text) => state.relay.prompt(&session, text).await.map(|()| None),
            None => {
                return action_error(StatusCode::BAD_REQUEST, "prompt needs `text`".to_string())
            }
        },
        other => {
            return action_error(
                StatusCode::BAD_REQUEST,
                format!("unknown action `{other}`; supported: focus, scroll, prompt"),
            )
        }
    };
    match result {
        Ok(offset_from_bottom) => {
            info!(action = %request.action, session = %session.id, "action relayed");
            (
                StatusCode::OK,
                Json(ActionResponse {
                    ok: true,
                    error: None,
                    offset_from_bottom,
                }),
            )
        }
        Err(err) => {
            warn!(action = %request.action, session = %session.id, error = %format!("{err:#}"), "action failed");
            action_error(StatusCode::BAD_GATEWAY, format!("{err:#}"))
        }
    }
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

    /// Recorded `agent.focus` reply from Herdr 0.9.4 (2026-10-18): the result
    /// is the focused agent's info, not `ok`.
    const AGENT_FOCUS_REPLY: &str = r#"{"type":"agent_info","agent":{"terminal_id":"term_65e1ea01c6a2915","agent":"claude","terminal_title":"◐ Coding policy plugin from Tessl","terminal_title_stripped":"Coding policy plugin from Tessl","agent_status":"working","tokens":{"model":"fable-5-1"},"agent_session":{"source":"herdr:claude","agent":"claude","kind":"id","value":"4827ac7d-2a1c-40c5-ae52-710fe42318c0"},"workspace_id":"w9","tab_id":"w9:t1","pane_id":"w9:p1","focused":true,"state_change_seq":4696,"cwd":"/Users/vikgamov/projects/lcars","foreground_cwd":"/Users/vikgamov/projects/lcars","revision":9}}"#;

    /// Fake Herdr for the ds9 host: answers `agent.focus` with the recorded
    /// `agent_info` reply, everything else with `ok`, and records the request.
    async fn fake_herdr(
        path: std::path::PathBuf,
        seen: tokio::sync::mpsc::UnboundedSender<serde_json::Value>,
    ) {
        use tokio::io::{AsyncBufReadExt, BufReader};
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let mut lines = BufReader::new(stream).lines();
            let Some(line) = lines.next_line().await.unwrap() else {
                continue;
            };
            let req: serde_json::Value = serde_json::from_str(&line).unwrap();
            let result: serde_json::Value = match req["method"].as_str().unwrap() {
                "agent.focus" => serde_json::from_str(AGENT_FOCUS_REPLY).unwrap(),
                "agent.prompt" => {
                    let mut v: serde_json::Value = serde_json::from_str(AGENT_FOCUS_REPLY).unwrap();
                    v["type"] = serde_json::json!("agent_prompted");
                    v
                }
                // Recorded pane shape (docs/fixtures): 40 rows of scrollback above.
                "pane.get" => serde_json::json!({"type": "pane_info", "pane": {
                    "pane_id": "w7:p1", "terminal_id": "t", "workspace_id": "w7", "tab_id": "w7:t1",
                    "focused": true, "agent_status": "blocked", "revision": 3,
                    "scroll": {"offset_from_bottom": 10, "max_offset_from_bottom": 40, "viewport_rows": 22}
                }}),
                _ => serde_json::json!({"type": "ok"}),
            };
            let reply = serde_json::json!({"id": req["id"], "result": result});
            seen.send(req).unwrap();
            let mut stream = lines.into_inner().into_inner();
            stream
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
        }
    }

    struct Harness {
        addr: std::net::SocketAddr,
        tx: watch::Sender<Arc<FleetView>>,
        herdr_requests: tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
    }

    static NEXT_SOCKET: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    async fn start() -> Harness {
        let fleet = fleet_with_one_blocked_session();
        let (tx, rx) = watch::channel(Arc::new(FleetView::from_fleet(&fleet)));
        let dir = std::env::temp_dir().join(format!("ops-s-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let n = NEXT_SOCKET.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let socket = dir.join(format!("h{n}.sock"));
        let _ = std::fs::remove_file(&socket);
        let (seen_tx, herdr_requests) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(fake_herdr(socket.clone(), seen_tx));
        let mut hosts = std::collections::HashMap::new();
        hosts.insert(
            "ds9".to_string(),
            crate::herdr::transport::Transport::Local { socket },
        );
        let relay = Arc::new(ActionRelay::new(hosts));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(serve(listener, rx, relay));
        Harness {
            addr,
            tx,
            herdr_requests,
        }
    }

    async fn post_action(addr: std::net::SocketAddr, body: &str) -> (u16, serde_json::Value) {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(
                format!(
                    "POST /actions HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).await.unwrap();
        let status: u16 = raw.split(' ').nth(1).unwrap().parse().unwrap();
        let json = serde_json::from_str(raw.split("\r\n\r\n").nth(1).unwrap()).unwrap();
        (status, json)
    }

    #[tokio::test]
    async fn focus_action_relays_agent_focus_to_the_owning_host() {
        let mut h = start().await;
        let (status, json) =
            post_action(h.addr, r#"{"action":"focus","session":"ds9/w7:p1"}"#).await;
        assert_eq!(status, 200, "{json}");
        assert_eq!(json["ok"], true);
        let req = h.herdr_requests.recv().await.unwrap();
        assert_eq!(req["method"], "agent.focus");
        assert_eq!(req["params"]["target"], "w7:p1");
    }

    #[tokio::test]
    async fn focus_on_a_plain_shell_uses_pane_focus() {
        let mut h = start().await;
        let mut fleet = fleet_with_one_blocked_session();
        fleet.apply(
            "ds9",
            HostEvent::StatusChanged(PaneAgentStatusChangedEvent {
                pane_id: "w7:p2".into(),
                workspace_id: "w7".into(),
                agent_status: herdr_compat::api::schema::AgentStatus::Unknown,
                agent: None,
                title: None,
                display_agent: None,
                state_labels: HashMap::new(),
            }),
            1_005,
        );
        h.tx.send(Arc::new(FleetView::from_fleet(&fleet))).unwrap();
        let (status, _) = post_action(h.addr, r#"{"action":"focus","session":"ds9/w7:p2"}"#).await;
        assert_eq!(status, 200);
        let req = h.herdr_requests.recv().await.unwrap();
        assert_eq!(req["method"], "pane.focus");
        assert_eq!(req["params"]["pane_id"], "w7:p2");
    }

    #[tokio::test]
    async fn scroll_reads_the_offset_then_sets_a_clamped_absolute_one() {
        let mut h = start().await;
        // 10 + 3 rows up = 13.
        let (status, json) = post_action(
            h.addr,
            r#"{"action":"scroll","session":"ds9/w7:p1","delta":3}"#,
        )
        .await;
        assert_eq!(status, 200, "{json}");
        assert_eq!(json["offset_from_bottom"], 13);
        let get = h.herdr_requests.recv().await.unwrap();
        assert_eq!(get["method"], "pane.get");
        let scroll = h.herdr_requests.recv().await.unwrap();
        assert_eq!(scroll["method"], "pane.scroll");
        assert_eq!(
            scroll["params"],
            serde_json::json!({"pane_id": "w7:p1", "offset_from_bottom": 13})
        );
        // Past the top clamps to max (40); past the bottom clamps to 0.
        let (_, json) = post_action(
            h.addr,
            r#"{"action":"scroll","session":"ds9/w7:p1","delta":500}"#,
        )
        .await;
        assert_eq!(json["offset_from_bottom"], 40);
        let (_, json) = post_action(
            h.addr,
            r#"{"action":"scroll","session":"ds9/w7:p1","delta":-500}"#,
        )
        .await;
        assert_eq!(json["offset_from_bottom"], 0);
        let (status, json) =
            post_action(h.addr, r#"{"action":"scroll","session":"ds9/w7:p1"}"#).await;
        assert_eq!(status, 400);
        assert!(json["error"].as_str().unwrap().contains("delta"));
    }

    #[tokio::test]
    async fn prompt_submits_text_to_the_agent_and_refuses_empty_or_agentless() {
        let mut h = start().await;
        let (status, json) = post_action(
            h.addr,
            r#"{"action":"prompt","session":"ds9/w7:p1","text":"/effort high"}"#,
        )
        .await;
        assert_eq!(status, 200, "{json}");
        let req = h.herdr_requests.recv().await.unwrap();
        assert_eq!(req["method"], "agent.prompt");
        assert_eq!(req["params"]["target"], "w7:p1");
        assert_eq!(req["params"]["text"], "/effort high");
        let (status, _) = post_action(
            h.addr,
            r#"{"action":"prompt","session":"ds9/w7:p1","text":"  "}"#,
        )
        .await;
        assert_eq!(status, 502);
        let (status, json) =
            post_action(h.addr, r#"{"action":"prompt","session":"ds9/w7:p1"}"#).await;
        assert_eq!(status, 400);
        assert!(json["error"].as_str().unwrap().contains("text"));
    }

    #[tokio::test]
    async fn unknown_session_is_404_and_unknown_action_is_400() {
        let h = start().await;
        let (status, json) =
            post_action(h.addr, r#"{"action":"focus","session":"ds9/w0:p0"}"#).await;
        assert_eq!(status, 404);
        assert!(json["error"].as_str().unwrap().contains("ds9/w0:p0"));
        let (status, json) =
            post_action(h.addr, r#"{"action":"teleport","session":"ds9/w7:p1"}"#).await;
        assert_eq!(status, 400);
        assert!(json["error"].as_str().unwrap().contains("teleport"));
    }

    #[tokio::test]
    async fn get_sessions_returns_hosts_and_sorted_sessions_as_json() {
        let addr = start().await.addr;
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
        let addr = start().await.addr;
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
        let Harness { addr, tx, .. } = start().await;
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
