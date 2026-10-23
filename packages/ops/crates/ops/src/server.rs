//! Plugin-facing server on loopback: `GET /fleet` for a snapshot,
//! `GET /fleet/ws` for a push stream, `POST /actions` to relay key presses,
//! `GET /healthz` unauthenticated. When Ops owns desktop banners, `/healthz`
//! carries `X-Ops-Banners: on` so herdr-notify knows to stay silent.
//!
//! Everything but `/healthz` requires `Authorization: Bearer <token>`; the
//! token lives in a 0600 file under the state dir that only the same user
//! (and therefore the plugin) can read. Loopback is shared by every account
//! on the machine, and `/actions` presses keys inside agents.
//!
//! Every WebSocket message is the full `FleetView`; the plugin repaints
//! from it. Views travel over a broadcast channel so a burst of changes
//! reaches the plugin one by one instead of collapsing to the last one.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, watch};
use tracing::{debug, info, warn};

use crate::actions::ActionRelay;
use crate::model::{AgentState, Fleet, Pane, SessionStatus, Workspace};
use crate::reveal::Revealer;

/// Version of the `FleetView` shape. 2: Herdr naming (`sessions` are Herdr
/// sessions, `panes` their panes), served on `/fleet`; version 1 was
/// `hosts` + `sessions` on `/sessions`.
pub const API_VERSION: u32 = 2;

/// What the plugin receives: Herdr sessions, their panes in display order
/// (blocked first, then most recently changed) and their workspaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FleetView {
    pub api: u32,
    pub sessions: Vec<SessionStatus>,
    pub panes: Vec<Pane>,
    #[serde(default)]
    pub workspaces: Vec<Workspace>,
}

impl FleetView {
    pub fn from_fleet(fleet: &Fleet) -> Self {
        FleetView {
            api: API_VERSION,
            sessions: fleet.sessions.values().cloned().collect(),
            panes: fleet.panes_sorted().into_iter().cloned().collect(),
            workspaces: fleet.workspaces_sorted().into_iter().cloned().collect(),
        }
    }
}

pub type FleetRx = watch::Receiver<Arc<FleetView>>;
pub type Updates = broadcast::Sender<Arc<FleetView>>;

/// Views buffered per WebSocket before the oldest is dropped.
pub const UPDATE_BUFFER: usize = 256;

/// Keys that interrupt or dismiss rather than confirm; allowed in any state.
pub const SAFE_KEYS: &[&str] = &["esc", "escape", "ctrl+c"];

#[derive(Clone)]
struct AppState {
    fleet: FleetRx,
    updates: Updates,
    relay: Arc<ActionRelay>,
    revealer: Arc<Revealer>,
    token: Arc<str>,
}

/// What the server offers beyond the fleet and the relay.
#[derive(Clone, Default)]
pub struct ServeOptions {
    /// Ops raises desktop banners; advertised on `/healthz`.
    pub banners: bool,
    /// Puts a focused local pane on screen after `focus`.
    pub revealer: Arc<Revealer>,
}

/// Header on `/healthz` that tells herdr-notify Ops owns banners.
pub const BANNERS_HEADER: &str = "x-ops-banners";

pub fn router(
    fleet: FleetRx,
    updates: Updates,
    relay: Arc<ActionRelay>,
    token: &str,
    options: ServeOptions,
) -> Router {
    let state = AppState {
        fleet,
        updates,
        relay,
        revealer: options.revealer,
        token: Arc::from(token),
    };
    let banners = options.banners;
    let protected = Router::new()
        .route("/fleet", get(fleet_view))
        .route("/actions", post(actions))
        .route("/fleet/ws", get(ws_upgrade))
        .layer(middleware::from_fn_with_state(state.clone(), require_token));
    Router::new()
        .route(
            "/healthz",
            get(move || async move {
                if banners {
                    ([(BANNERS_HEADER, "on")], "ok").into_response()
                } else {
                    "ok".into_response()
                }
            }),
        )
        .merge(protected)
        .with_state(state)
}

/// Serve until the listener fails. Caller owns binding so tests can use port 0.
pub async fn serve(
    listener: TcpListener,
    fleet: FleetRx,
    updates: Updates,
    relay: Arc<ActionRelay>,
    token: &str,
    options: ServeOptions,
) -> std::io::Result<()> {
    info!(addr = %listener.local_addr()?, "serving plugin API");
    axum::serve(listener, router(fleet, updates, relay, token, options)).await
}

/// Path of the API token file inside the state dir.
pub fn token_path(state_dir: &Path) -> PathBuf {
    state_dir.join("token")
}

/// Read the token file, or create one (32 random bytes, hex, mode 0600).
pub fn load_or_create_token(state_dir: &Path) -> std::io::Result<String> {
    let path = token_path(state_dir);
    if let Ok(existing) = std::fs::read_to_string(&path) {
        let trimmed = existing.trim().to_string();
        if trimmed.len() >= 32 {
            return Ok(trimmed);
        }
    }
    std::fs::create_dir_all(state_dir)?;
    let mut bytes = [0u8; 32];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&path)?;
    file.write_all(token.as_bytes())?;
    Ok(token)
}

async fn require_token(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let presented = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .map(str::trim);
    if presented == Some(&*state.token) {
        return next.run(request).await;
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(ActionResponse {
            ok: false,
            error: Some(
                "missing or wrong bearer token; Ops writes it to <state dir>/token".to_string(),
            ),
            offset_from_bottom: None,
            revealed: None,
        }),
    )
        .into_response()
}

/// `POST /actions` body. `pane` is a `FleetView` pane id (`session/pane`).
///
/// Actions: `focus`; `scroll` with `delta` rows (positive scrolls up into
/// scrollback); `prompt` with `text` submitted to the agent plus Enter;
/// `send_keys` with `keys` (Herdr logical key names such as `enter`, `esc`);
/// `focus_workspace` with `workspace` (`session/w1`) instead of `pane`.
/// Keys outside `SAFE_KEYS` are relayed only to a `blocked` pane.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionRequest {
    pub action: String,
    #[serde(default)]
    pub pane: String,
    #[serde(default)]
    pub workspace: Option<String>,
    #[serde(default)]
    pub delta: Option<i64>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub keys: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// `scroll`: the offset from bottom that was applied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_from_bottom: Option<u64>,
    /// `focus`: whether Ops revealed the iTerm session showing the pane. When
    /// false the caller raises the terminal app itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revealed: Option<bool>,
}

fn action_error(status: StatusCode, error: String) -> (StatusCode, Json<ActionResponse>) {
    (
        status,
        Json(ActionResponse {
            ok: false,
            error: Some(error),
            offset_from_bottom: None,
            revealed: None,
        }),
    )
}

async fn fleet_view(State(state): State<AppState>) -> Json<FleetView> {
    let view: FleetView = (**state.fleet.borrow()).clone();
    Json(view)
}

async fn actions(
    State(state): State<AppState>,
    Json(request): Json<ActionRequest>,
) -> (StatusCode, Json<ActionResponse>) {
    if request.action == "focus_workspace" {
        return focus_workspace(&state, request.workspace.as_deref().unwrap_or("")).await;
    }
    let (pane, session) = {
        let view = state.fleet.borrow();
        let pane = view.panes.iter().find(|s| s.id == request.pane).cloned();
        let session = pane
            .as_ref()
            .and_then(|s| view.sessions.iter().find(|h| h.name == s.session).cloned());
        (pane, session)
    };
    let Some(pane) = pane else {
        return action_error(
            StatusCode::NOT_FOUND,
            format!(
                "no pane `{}` — the key may be stale, wait for the next update",
                request.pane
            ),
        );
    };
    if let Some(session) = session {
        if !session.connected {
            return action_error(
                StatusCode::SERVICE_UNAVAILABLE,
                format!(
                    "session `{}` is disconnected ({}); the key shows its last known state",
                    session.name,
                    session.error.as_deref().unwrap_or("no detail")
                ),
            );
        }
    }
    let mut revealed = None;
    let result: anyhow::Result<Option<u64>> = match request.action.as_str() {
        "focus" => match state.relay.focus(&pane).await {
            Ok(()) => {
                revealed = Some(match state.revealer.reveal(&pane.session).await {
                    Ok(done) => done,
                    Err(err) => {
                        warn!(pane = %pane.id, error = %format!("{err:#}"), "reveal failed");
                        false
                    }
                });
                Ok(None)
            }
            Err(err) => Err(err),
        },
        "scroll" => match request.delta {
            Some(delta) => state.relay.scroll(&pane, delta).await.map(Some),
            None => {
                return action_error(
                    StatusCode::BAD_REQUEST,
                    "scroll needs `delta` (rows, positive = up)".to_string(),
                )
            }
        },
        "prompt" => match request.text.as_deref() {
            Some(text) => state.relay.prompt(&pane, text).await.map(|()| None),
            None => {
                return action_error(StatusCode::BAD_REQUEST, "prompt needs `text`".to_string())
            }
        },
        "send_keys" => match request.keys.as_deref() {
            Some(keys) => {
                let confirming = keys.iter().any(|k| !SAFE_KEYS.contains(&k.as_str()));
                if confirming && pane.state != AgentState::Blocked {
                    return action_error(
                        StatusCode::CONFLICT,
                        format!(
                            "refusing {:?} on {}: it is {:?}, not blocked; only {:?} may be sent to a non-blocked pane",
                            keys, pane.id, pane.state, SAFE_KEYS
                        ),
                    );
                }
                state.relay.send_keys(&pane, keys).await.map(|()| None)
            }
            None => {
                return action_error(
                    StatusCode::BAD_REQUEST,
                    "send_keys needs `keys` (e.g. [\"enter\"])".to_string(),
                )
            }
        },
        other => {
            return action_error(
                StatusCode::BAD_REQUEST,
                format!("unknown action `{other}`; supported: focus, scroll, prompt, send_keys"),
            )
        }
    };
    match result {
        Ok(offset_from_bottom) => {
            info!(action = %request.action, pane = %pane.id, "action relayed");
            (
                StatusCode::OK,
                Json(ActionResponse {
                    ok: true,
                    error: None,
                    offset_from_bottom,
                    revealed,
                }),
            )
        }
        Err(err) => {
            warn!(action = %request.action, pane = %pane.id, error = %format!("{err:#}"), "action failed");
            action_error(StatusCode::BAD_GATEWAY, format!("{err:#}"))
        }
    }
}

async fn focus_workspace(state: &AppState, id: &str) -> (StatusCode, Json<ActionResponse>) {
    let (workspace, session) = {
        let view = state.fleet.borrow();
        let workspace = view.workspaces.iter().find(|w| w.id == id).cloned();
        let session = workspace
            .as_ref()
            .and_then(|w| view.sessions.iter().find(|h| h.name == w.session).cloned());
        (workspace, session)
    };
    let Some(workspace) = workspace else {
        return action_error(
            StatusCode::NOT_FOUND,
            format!("no workspace `{id}` — the key may be stale, wait for the next update"),
        );
    };
    if let Some(session) = session {
        if !session.connected {
            return action_error(
                StatusCode::SERVICE_UNAVAILABLE,
                format!("session `{}` is disconnected", session.name),
            );
        }
    }
    match state.relay.focus_workspace(&workspace).await {
        Ok(()) => {
            info!(workspace = %workspace.id, "workspace focused");
            (
                StatusCode::OK,
                Json(ActionResponse {
                    ok: true,
                    error: None,
                    offset_from_bottom: None,
                    revealed: None,
                }),
            )
        }
        Err(err) => {
            warn!(workspace = %workspace.id, error = %format!("{err:#}"), "workspace focus failed");
            action_error(StatusCode::BAD_GATEWAY, format!("{err:#}"))
        }
    }
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_stream(socket, state.fleet, state.updates.subscribe()))
}

async fn ws_stream(
    mut socket: WebSocket,
    fleet: FleetRx,
    mut updates: broadcast::Receiver<Arc<FleetView>>,
) {
    debug!("plugin websocket connected");
    // Current view first, then every published change in order.
    let initial = fleet.borrow().clone();
    if send_view(&mut socket, &initial).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            next = updates.recv() => {
                let view = match next {
                    Ok(view) => view,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        // Too slow for the burst: the current view still holds every state.
                        warn!(skipped = n, "plugin websocket lagged; sending the current view");
                        fleet.borrow().clone()
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                };
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

/// Publish a new view: update the snapshot and broadcast it to every socket.
pub fn publish(view_tx: &watch::Sender<Arc<FleetView>>, updates: &Updates, view: Arc<FleetView>) {
    let _ = view_tx.send(view.clone());
    let _ = updates.send(view); // no subscribers is not an error
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionEvent;
    use futures_util::StreamExt;
    use herdr_compat::api::schema::{AgentStatus, PaneAgentStatusChangedEvent};
    use std::collections::HashMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;

    const TOKEN: &str = "test-token-0123456789abcdef0123456789abcdef";

    fn status_event(pane: &str, status: AgentStatus, agent: Option<&str>) -> SessionEvent {
        SessionEvent::StatusChanged(PaneAgentStatusChangedEvent {
            pane_id: pane.into(),
            workspace_id: "w7".into(),
            agent_status: status,
            agent: agent.map(str::to_string),
            title: Some("approve?".into()),
            display_agent: None,
            state_labels: HashMap::new(),
        })
    }

    fn fleet_with_one_blocked_session() -> Fleet {
        let mut fleet = Fleet::with_sessions([("mini", Some("mini"))], 1_000);
        fleet.apply(
            "mini",
            SessionEvent::Connected {
                version: "0.9.0".into(),
                protocol: 22,
            },
            1_001,
        );
        fleet.apply(
            "mini",
            status_event("w7:p1", AgentStatus::Blocked, Some("claude")),
            1_002,
        );
        let ws: herdr_compat::api::schema::WorkspaceInfo =
            serde_json::from_value(serde_json::json!({
                "workspace_id": "w7", "number": 1, "label": "watch", "focused": true,
                "pane_count": 2, "tab_count": 1, "active_tab_id": "w7:t1", "agent_status": "blocked"
            }))
            .unwrap();
        fleet.apply("mini", SessionEvent::Workspaces(vec![ws]), 1_003);
        fleet
    }

    /// Recorded `agent.focus` reply from Herdr 0.9.4 (2026-10-18): the result
    /// is the focused agent's info, not `ok`.
    const AGENT_FOCUS_REPLY: &str = r#"{"type":"agent_info","agent":{"terminal_id":"term_65e1ea01c6a2915","agent":"claude","terminal_title":"◐ Coding policy plugin from Tessl","terminal_title_stripped":"Coding policy plugin from Tessl","agent_status":"working","tokens":{"model":"fable-5-1"},"agent_session":{"source":"herdr:claude","agent":"claude","kind":"id","value":"4827ac7d-2a1c-40c5-ae52-710fe42318c0"},"workspace_id":"w9","tab_id":"w9:t1","pane_id":"w9:p1","focused":true,"state_change_seq":4696,"cwd":"/Users/me/projects/lcars","foreground_cwd":"/Users/me/projects/lcars","revision":9}}"#;

    /// Fake Herdr for the mini session: answers `agent.focus` with the recorded
    /// `agent_info` reply, everything else with a fitting result, and records the request.
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
        updates: Updates,
        herdr_requests: tokio::sync::mpsc::UnboundedReceiver<serde_json::Value>,
    }

    impl Harness {
        fn publish(&self, fleet: &Fleet) {
            publish(
                &self.tx,
                &self.updates,
                Arc::new(FleetView::from_fleet(fleet)),
            );
        }
    }

    static NEXT_SOCKET: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    async fn start() -> Harness {
        start_with(ServeOptions::default()).await
    }

    async fn start_with(options: ServeOptions) -> Harness {
        let fleet = fleet_with_one_blocked_session();
        let (tx, rx) = watch::channel(Arc::new(FleetView::from_fleet(&fleet)));
        let (updates, _) = broadcast::channel(UPDATE_BUFFER);
        let dir = std::env::temp_dir().join(format!("ops-s-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let n = NEXT_SOCKET.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let socket = dir.join(format!("h{n}.sock"));
        let _ = std::fs::remove_file(&socket);
        let (seen_tx, herdr_requests) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(fake_herdr(socket.clone(), seen_tx));
        let mut sessions = std::collections::HashMap::new();
        sessions.insert(
            "mini".to_string(),
            crate::herdr::transport::Transport::Local { socket },
        );
        let relay = Arc::new(ActionRelay::new(sessions));
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let serve_updates = updates.clone();
        tokio::spawn(
            async move { serve(listener, rx, serve_updates, relay, TOKEN, options).await },
        );
        Harness {
            addr,
            tx,
            updates,
            herdr_requests,
        }
    }

    async fn http(addr: std::net::SocketAddr, request: &str) -> (u16, String) {
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut raw = String::new();
        stream.read_to_string(&mut raw).await.unwrap();
        let status: u16 = raw.split(' ').nth(1).unwrap().parse().unwrap();
        let body = raw.split("\r\n\r\n").nth(1).unwrap_or("").to_string();
        (status, body)
    }

    async fn get(addr: std::net::SocketAddr, path: &str, token: Option<&str>) -> (u16, String) {
        let auth = token
            .map(|t| format!("Authorization: Bearer {t}\r\n"))
            .unwrap_or_default();
        http(
            addr,
            &format!("GET {path} HTTP/1.1\r\nHost: {addr}\r\n{auth}Connection: close\r\n\r\n"),
        )
        .await
    }

    async fn post_action(addr: std::net::SocketAddr, body: &str) -> (u16, serde_json::Value) {
        let (status, text) = http(
            addr,
            &format!(
                "POST /actions HTTP/1.1\r\nHost: {addr}\r\nAuthorization: Bearer {TOKEN}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            ),
        )
        .await;
        (status, serde_json::from_str(&text).unwrap())
    }

    type Ws = tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >;

    async fn ws_connect(addr: std::net::SocketAddr) -> Ws {
        let mut request = format!("ws://{addr}/fleet/ws")
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("Authorization", format!("Bearer {TOKEN}").parse().unwrap());
        tokio_tungstenite::connect_async(request).await.unwrap().0
    }

    #[tokio::test]
    async fn get_fleet_returns_sessions_and_sorted_panes_as_json() {
        let h = start().await;
        let (status, body) = get(h.addr, "/fleet", Some(TOKEN)).await;
        assert_eq!(status, 200, "{body}");
        let view: FleetView = serde_json::from_str(&body).unwrap();
        assert_eq!(view.sessions.len(), 1);
        assert!(view.sessions[0].connected);
        assert_eq!(view.panes.len(), 1);
        assert_eq!(view.panes[0].id, "mini/w7:p1");
        assert_eq!(view.panes[0].state, AgentState::Blocked);
        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(json["api"], 2);
        assert_eq!(json["sessions"][0]["name"], "mini");
        assert_eq!(json["sessions"][0]["machine"], "mini");
        assert_eq!(json["panes"][0]["session"], "mini");
        assert_eq!(json["panes"][0]["state"], "blocked");
    }

    #[tokio::test]
    async fn version_one_routes_are_gone() {
        // A plugin from before the rename must see "no Ops", not a view it
        // would misread.
        let h = start().await;
        let (status, _) = get(h.addr, "/sessions", Some(TOKEN)).await;
        assert_eq!(status, 404);
        let (status, _) = get(h.addr, "/ws", Some(TOKEN)).await;
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn healthz_is_open_and_everything_else_needs_the_token() {
        let h = start().await;
        let (status, body) = get(h.addr, "/healthz", None).await;
        assert_eq!(status, 200);
        assert_eq!(body, "ok");
        let (status, _) = get(h.addr, "/fleet", None).await;
        assert_eq!(status, 401);
        let (status, _) = get(h.addr, "/fleet", Some("wrong")).await;
        assert_eq!(status, 401);
        let body = r#"{"action":"focus","pane":"mini/w7:p1"}"#;
        let (status, text) = http(
            h.addr,
            &format!(
                "POST /actions HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                h.addr,
                body.len()
            ),
        )
        .await;
        assert_eq!(status, 401, "{text}");
        assert!(text.contains("bearer token"));
        let (status, _) = get(h.addr, "/fleet/ws", None).await;
        assert_eq!(status, 401);
    }

    async fn healthz_raw(addr: std::net::SocketAddr) -> String {
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
        raw.to_ascii_lowercase()
    }

    #[tokio::test]
    async fn healthz_advertises_banners_only_when_ops_owns_them() {
        let off = start().await;
        assert!(!healthz_raw(off.addr).await.contains("x-ops-banners"));
        let on = start_with(ServeOptions {
            banners: true,
            ..ServeOptions::default()
        })
        .await;
        let raw = healthz_raw(on.addr).await;
        assert!(raw.contains("\r\nx-ops-banners: on\r\n"), "{raw}");
        assert!(raw.ends_with("\r\n\r\nok"), "{raw}");
    }

    #[tokio::test]
    async fn focus_action_relays_agent_focus_to_the_owning_host() {
        let mut h = start().await;
        let (status, json) = post_action(h.addr, r#"{"action":"focus","pane":"mini/w7:p1"}"#).await;
        assert_eq!(status, 200, "{json}");
        assert_eq!(json["ok"], true);
        // No local socket configured for the revealer: the plugin raises the app.
        assert_eq!(json["revealed"], false);
        let req = h.herdr_requests.recv().await.unwrap();
        assert_eq!(req["method"], "agent.focus");
        assert_eq!(req["params"]["target"], "w7:p1");
    }

    #[tokio::test]
    async fn focus_on_a_plain_shell_uses_pane_focus() {
        let mut h = start().await;
        let mut fleet = fleet_with_one_blocked_session();
        fleet.apply(
            "mini",
            status_event("w7:p2", AgentStatus::Unknown, None),
            1_005,
        );
        h.publish(&fleet);
        let (status, _) = post_action(h.addr, r#"{"action":"focus","pane":"mini/w7:p2"}"#).await;
        assert_eq!(status, 200);
        let req = h.herdr_requests.recv().await.unwrap();
        assert_eq!(req["method"], "pane.focus");
        assert_eq!(req["params"]["pane_id"], "w7:p2");
    }

    #[tokio::test]
    async fn scroll_reads_the_offset_then_sets_a_clamped_absolute_one() {
        let mut h = start().await;
        let (status, json) = post_action(
            h.addr,
            r#"{"action":"scroll","pane":"mini/w7:p1","delta":3}"#,
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
        let (_, json) = post_action(
            h.addr,
            r#"{"action":"scroll","pane":"mini/w7:p1","delta":500}"#,
        )
        .await;
        assert_eq!(json["offset_from_bottom"], 40);
        let (_, json) = post_action(
            h.addr,
            r#"{"action":"scroll","pane":"mini/w7:p1","delta":-500}"#,
        )
        .await;
        assert_eq!(json["offset_from_bottom"], 0);
        let (status, json) =
            post_action(h.addr, r#"{"action":"scroll","pane":"mini/w7:p1"}"#).await;
        assert_eq!(status, 400);
        assert!(json["error"].as_str().unwrap().contains("delta"));
    }

    #[tokio::test]
    async fn prompt_submits_text_to_the_agent_and_refuses_empty_or_agentless() {
        let mut h = start().await;
        let (status, json) = post_action(
            h.addr,
            r#"{"action":"prompt","pane":"mini/w7:p1","text":"/effort high"}"#,
        )
        .await;
        assert_eq!(status, 200, "{json}");
        let req = h.herdr_requests.recv().await.unwrap();
        assert_eq!(req["method"], "agent.prompt");
        assert_eq!(req["params"]["target"], "w7:p1");
        assert_eq!(req["params"]["text"], "/effort high");
        let (status, _) = post_action(
            h.addr,
            r#"{"action":"prompt","pane":"mini/w7:p1","text":"  "}"#,
        )
        .await;
        assert_eq!(status, 502);
        let (status, json) =
            post_action(h.addr, r#"{"action":"prompt","pane":"mini/w7:p1"}"#).await;
        assert_eq!(status, 400);
        assert!(json["error"].as_str().unwrap().contains("text"));
    }

    #[tokio::test]
    async fn send_keys_confirms_only_blocked_panes_but_escape_always_passes() {
        let mut h = start().await;
        let (status, json) = post_action(
            h.addr,
            r#"{"action":"send_keys","pane":"mini/w7:p1","keys":["enter"]}"#,
        )
        .await;
        assert_eq!(status, 200, "{json}");
        let req = h.herdr_requests.recv().await.unwrap();
        assert_eq!(req["method"], "agent.send_keys");
        assert_eq!(
            req["params"],
            serde_json::json!({"target": "w7:p1", "keys": ["enter"]})
        );

        let mut fleet = fleet_with_one_blocked_session();
        fleet.apply(
            "mini",
            status_event("w7:p3", AgentStatus::Working, Some("codex")),
            1_006,
        );
        h.publish(&fleet);
        let (status, json) = post_action(
            h.addr,
            r#"{"action":"send_keys","pane":"mini/w7:p3","keys":["enter"]}"#,
        )
        .await;
        assert_eq!(status, 409, "{json}");
        assert!(json["error"].as_str().unwrap().contains("not blocked"));
        let (status, _) = post_action(
            h.addr,
            r#"{"action":"send_keys","pane":"mini/w7:p3","keys":["esc"]}"#,
        )
        .await;
        assert_eq!(status, 200);
        let req = h.herdr_requests.recv().await.unwrap();
        assert_eq!(req["params"]["keys"], serde_json::json!(["esc"]));

        let (status, json) = post_action(
            h.addr,
            r#"{"action":"send_keys","pane":"mini/w7:p1","keys":[]}"#,
        )
        .await;
        assert_eq!(status, 502, "{json}");
        let (status, _) =
            post_action(h.addr, r#"{"action":"send_keys","pane":"mini/w7:p1"}"#).await;
        assert_eq!(status, 400);
    }

    #[tokio::test]
    async fn actions_on_a_disconnected_host_are_refused() {
        let h = start().await;
        let mut fleet = fleet_with_one_blocked_session();
        fleet.apply(
            "mini",
            SessionEvent::Disconnected {
                error: "ssh master gone".into(),
            },
            1_007,
        );
        h.publish(&fleet);
        let (status, json) = post_action(h.addr, r#"{"action":"focus","pane":"mini/w7:p1"}"#).await;
        assert_eq!(status, 503, "{json}");
        assert!(json["error"].as_str().unwrap().contains("ssh master gone"));
    }

    #[tokio::test]
    async fn focus_workspace_relays_workspace_focus_and_rejects_unknown_ids() {
        let mut h = start().await;
        let (status, body) = get(h.addr, "/fleet", Some(TOKEN)).await;
        assert_eq!(status, 200);
        let view: FleetView = serde_json::from_str(&body).unwrap();
        assert_eq!(view.workspaces.len(), 1);
        assert_eq!(view.workspaces[0].id, "mini/w7");
        assert_eq!(view.workspaces[0].label, "watch");
        let (status, json) = post_action(
            h.addr,
            r#"{"action":"focus_workspace","workspace":"mini/w7"}"#,
        )
        .await;
        assert_eq!(status, 200, "{json}");
        let req = h.herdr_requests.recv().await.unwrap();
        assert_eq!(req["method"], "workspace.focus");
        assert_eq!(req["params"]["workspace_id"], "w7");
        let (status, _) = post_action(
            h.addr,
            r#"{"action":"focus_workspace","workspace":"mini/w0"}"#,
        )
        .await;
        assert_eq!(status, 404);
    }

    #[tokio::test]
    async fn unknown_session_is_404_and_unknown_action_is_400() {
        let h = start().await;
        let (status, json) = post_action(h.addr, r#"{"action":"focus","pane":"mini/w0:p0"}"#).await;
        assert_eq!(status, 404);
        assert!(json["error"].as_str().unwrap().contains("mini/w0:p0"));
        let (status, json) =
            post_action(h.addr, r#"{"action":"teleport","pane":"mini/w7:p1"}"#).await;
        assert_eq!(status, 400);
        assert!(json["error"].as_str().unwrap().contains("teleport"));
    }

    #[tokio::test]
    async fn websocket_sends_current_view_then_every_change_in_a_burst() {
        let h = start().await;
        let mut ws = ws_connect(h.addr).await;
        let first = ws.next().await.unwrap().unwrap().into_text().unwrap();
        let view: FleetView = serde_json::from_str(&first).unwrap();
        assert_eq!(view.panes[0].state, AgentState::Blocked);

        // Three changes published back to back, before the socket task runs:
        // working, blocked, idle must all arrive, in order.
        let mut fleet = fleet_with_one_blocked_session();
        let mut expected = Vec::new();
        for (i, s) in [
            AgentStatus::Working,
            AgentStatus::Blocked,
            AgentStatus::Idle,
        ]
        .into_iter()
        .enumerate()
        {
            fleet.apply(
                "mini",
                status_event("w7:p1", s, Some("claude")),
                1_010 + i as u64,
            );
            expected.push(AgentState::from(s));
            h.publish(&fleet);
        }
        let mut seen = Vec::new();
        for _ in 0..3 {
            let text = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap()
                .into_text()
                .unwrap();
            let view: FleetView = serde_json::from_str(&text).unwrap();
            seen.push(view.panes[0].state);
        }
        assert_eq!(seen, expected);
    }

    #[test]
    fn token_file_is_created_once_with_owner_only_permissions() {
        let dir = std::env::temp_dir().join(format!("ops-token-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let first = load_or_create_token(&dir).unwrap();
        assert_eq!(first.len(), 64);
        let second = load_or_create_token(&dir).unwrap();
        assert_eq!(first, second, "an existing token is reused");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(token_path(&dir))
                .unwrap()
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(mode, 0o600);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
