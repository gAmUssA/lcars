//! Per-session connection loop: ping, snapshot, subscribe, re-subscribe on
//! new panes, reconnect with backoff. Emits `SessionEvent`s; never touches
//! the merged model directly.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use herdr_compat::api::schema::{
    AgentReadParams, AgentStatus, AgentTarget, EmptyParams, EventData, EventsSubscribeParams,
    Method, PaneTarget, PingParams, ReadFormat, ReadSource, Request, ResponseResult, Subscription,
    SubscriptionEventData,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::transport::Transport;
use super::wire::{encode_request, parse_frame, Frame};
use crate::model::{extract_prompt, SessionEvent};
use crate::HERDR_PROTOCOL;

/// Reconnect backoff: first retry after `INITIAL`, doubling to `MAX`.
pub const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
pub const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// How long one request may take before the session counts as down.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
/// Interval of the out-of-band ping that catches a wedged forward while the
/// event stream is legitimately quiet.
pub const HEALTH_INTERVAL: Duration = Duration::from_secs(30);

enum Refresh {
    Pane(String),
    Agent(String),
    /// Read the visible screen of a pane that just blocked, for its question.
    Prompt(String),
}

/// Rows of screen read for a blocked pane's question.
pub const PROMPT_READ_LINES: u32 = 40;

/// Events from one session, tagged with its name.
pub type SessionEventSender = mpsc::Sender<(String, SessionEvent)>;

pub struct SessionClient {
    pub name: String,
    pub transport: Transport,
    request_seq: u64,
}

impl SessionClient {
    pub fn new(name: impl Into<String>, transport: Transport) -> Self {
        SessionClient {
            name: name.into(),
            transport,
            request_seq: 0,
        }
    }

    /// Run forever: connect, stream events, reconnect with backoff.
    /// Ends only when the receiver is dropped.
    pub async fn run(mut self, tx: SessionEventSender) {
        let mut backoff = BACKOFF_INITIAL;
        loop {
            match self.connection(&tx, &mut backoff).await {
                Ok(()) => return,
                Err(err) => {
                    let message = format!("{err:#}");
                    warn!(session = %self.name, error = %message, "herdr connection lost");
                    if tx
                        .send((
                            self.name.clone(),
                            SessionEvent::Disconnected { error: message },
                        ))
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
            }
            if tx.is_closed() {
                return;
            }
            // A wedged forward still accepts connections; drop it so the next
            // connect goes through cancel-then-forward instead of reusing it.
            self.transport.reset().await;
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
    }

    /// One connection. Returns `Ok` only if the receiver went away;
    /// any loss of the event stream is an `Err` so the caller backs off.
    async fn connection(&mut self, tx: &SessionEventSender, backoff: &mut Duration) -> Result<()> {
        let pong = self.request(Method::Ping(PingParams::default())).await?;
        let (version, protocol) = match pong {
            ResponseResult::Pong {
                version, protocol, ..
            } => (version, protocol),
            other => bail!("ping returned {other:?} instead of pong"),
        };
        if protocol != HERDR_PROTOCOL {
            bail!(
                "Herdr {version} on {} speaks protocol {protocol}; Ops supports {HERDR_PROTOCOL}. Update Herdr there or rebuild Ops against the matching herdr-compat",
                self.name
            );
        }
        info!(session = %self.name, version = %version, protocol, via = %self.transport.describe(), "connected to herdr");
        // A successful ping is a healthy session: the next outage starts the
        // backoff ladder from the bottom again.
        *backoff = BACKOFF_INITIAL;
        if tx
            .send((
                self.name.clone(),
                SessionEvent::Connected { version, protocol },
            ))
            .await
            .is_err()
        {
            return Ok(());
        }

        let mut pane_ids = self.snapshot_and_publish(tx).await?;
        let mut lines = self.subscribe(&pane_ids).await?;
        let mut health = tokio::time::interval(HEALTH_INTERVAL);
        health.tick().await; // the first tick fires immediately; skip it
        loop {
            let line = tokio::select! {
                read = lines.next_line() => match read.context("reading herdr event stream")? {
                    Some(line) => line,
                    None => bail!("herdr closed the event stream"),
                },
                _ = health.tick() => {
                    // Separate connection: a stalled forward or a dead Herdr
                    // fails here even while the event socket stays silent.
                    self.request(Method::Ping(PingParams::default()))
                        .await
                        .context("health ping failed")?;
                    continue;
                }
            };
            let frame = match parse_frame(&line) {
                Ok(frame) => frame,
                Err(err) => {
                    warn!(session = %self.name, %err, line = %line, "unparseable frame, skipping");
                    continue;
                }
            };
            let Some(event) = dispatch(frame) else {
                continue;
            };

            match &event {
                // A new pane needs its own status subscription; Herdr has no
                // "add subscription" call, so open a replacement connection.
                // Frames buffered on the old connection are gone with it, so a
                // fresh snapshot follows every swap.
                SessionEvent::PaneCreated(pane) if !pane_ids.contains(&pane.pane_id) => {
                    if tx.send((self.name.clone(), event.clone())).await.is_err() {
                        return Ok(());
                    }
                    pane_ids.push(pane.pane_id.clone());
                    lines = match self.subscribe(&pane_ids).await {
                        Ok(fresh) => fresh,
                        Err(err) => {
                            // Usually a pane that closed while we were not
                            // listening: rebuild the set from the server.
                            warn!(session = %self.name, %err, "re-subscribe failed; rebuilding pane set from a snapshot");
                            pane_ids = self.snapshot_and_publish(tx).await?;
                            self.subscribe(&pane_ids).await?
                        }
                    };
                    pane_ids = self.snapshot_and_publish(tx).await?;
                    continue;
                }
                SessionEvent::PaneClosed { pane_id } => pane_ids.retain(|id| id != pane_id),
                // Workspace lists are small; a snapshot is the simplest truth.
                SessionEvent::WorkspacesChanged => {
                    pane_ids = self.snapshot_and_publish(tx).await?;
                    continue;
                }
                // Follow-up reads that the event stream cannot carry run on
                // their own task so a slow session never stalls the stream.
                SessionEvent::AgentDetected {
                    pane_id,
                    released: false,
                    ..
                } => self.spawn_refresh(tx, Refresh::Pane(pane_id.clone())),
                SessionEvent::StatusChanged(e) if e.agent_status == AgentStatus::Idle => {
                    self.spawn_refresh(tx, Refresh::Agent(e.pane_id.clone()))
                }
                SessionEvent::StatusChanged(e) if e.agent_status == AgentStatus::Blocked => {
                    self.spawn_refresh(tx, Refresh::Prompt(e.pane_id.clone()))
                }
                _ => {}
            }
            if tx.send((self.name.clone(), event)).await.is_err() {
                return Ok(());
            }
        }
    }

    /// `session.snapshot`, published as a full replacement; returns the pane ids.
    async fn snapshot_and_publish(&mut self, tx: &SessionEventSender) -> Result<Vec<String>> {
        let snapshot = match self
            .request(Method::SessionSnapshot(EmptyParams::default()))
            .await?
        {
            ResponseResult::SessionSnapshot { snapshot } => snapshot,
            other => bail!("session.snapshot returned {other:?}"),
        };
        let pane_ids: Vec<String> = snapshot.panes.iter().map(|p| p.pane_id.clone()).collect();
        let _ = tx
            .send((
                self.name.clone(),
                SessionEvent::Snapshot {
                    panes: snapshot.panes,
                    agents: snapshot.agents,
                },
            ))
            .await;
        let _ = tx
            .send((
                self.name.clone(),
                SessionEvent::Workspaces(snapshot.workspaces),
            ))
            .await;
        Ok(pane_ids)
    }

    fn spawn_refresh(&mut self, tx: &SessionEventSender, what: Refresh) {
        let transport = self.transport.clone();
        let tx = tx.clone();
        let session = self.name.clone();
        let id = self.next_id();
        tokio::spawn(async move {
            let (method, label) = match &what {
                Refresh::Pane(pane_id) => (
                    Method::PaneGet(PaneTarget {
                        pane_id: pane_id.clone(),
                    }),
                    "pane.get",
                ),
                Refresh::Agent(pane_id) => (
                    Method::AgentGet(AgentTarget {
                        target: pane_id.clone(),
                    }),
                    "agent.get",
                ),
                Refresh::Prompt(pane_id) => (
                    Method::AgentRead(AgentReadParams {
                        target: pane_id.clone(),
                        source: ReadSource::Visible,
                        lines: Some(PROMPT_READ_LINES),
                        format: ReadFormat::Text,
                        strip_ansi: true,
                    }),
                    "agent.read",
                ),
            };
            let event = match request_once(&transport, id, method).await {
                Ok(ResponseResult::PaneInfo { pane }) => {
                    SessionEvent::PaneRefreshed(Box::new(pane))
                }
                Ok(ResponseResult::AgentInfo { agent }) => {
                    SessionEvent::AgentRefreshed(Box::new(agent))
                }
                Ok(ResponseResult::PaneRead { read }) => SessionEvent::PromptRead {
                    pane_id: read.pane_id.clone(),
                    prompt: extract_prompt(&read.text),
                },
                Ok(other) => {
                    debug!(session = %session, %label, ?other, "unexpected result, ignoring");
                    return;
                }
                // The agent may already be gone (released, pane closed): not an error.
                Err(err) => {
                    debug!(session = %session, %label, %err, "follow-up read failed, ignoring");
                    return;
                }
            };
            let _ = tx.send((session, event)).await;
        });
    }

    fn next_id(&mut self) -> String {
        self.request_seq += 1;
        format!("ops:{}:{}", self.name, self.request_seq)
    }

    /// One request on a fresh connection; Herdr closes it after replying.
    async fn request(&mut self, method: Method) -> Result<ResponseResult> {
        let id = self.next_id();
        request_once(&self.transport, id, method).await
    }

    /// Open the long-lived subscription connection for the given panes.
    async fn subscribe(&mut self, pane_ids: &[String]) -> Result<Lines<BufReader<UnixStream>>> {
        let mut subscriptions = vec![
            Subscription::PaneCreated {},
            Subscription::PaneClosed {},
            Subscription::PaneAgentDetected {},
            Subscription::PaneFocused {},
            // Titles and cwd move during a turn; the merge ignores frames that
            // change nothing the deck shows (scroll, output).
            Subscription::PaneUpdated {},
            Subscription::WorkspaceCreated {},
            Subscription::WorkspaceClosed {},
            Subscription::WorkspaceRenamed {},
            Subscription::WorkspaceFocused {},
        ];
        subscriptions.extend(
            pane_ids
                .iter()
                .map(|pane_id| Subscription::PaneAgentStatusChanged {
                    pane_id: pane_id.clone(),
                    agent_status: None,
                }),
        );
        let request = Request {
            id: self.next_id(),
            method: Method::EventsSubscribe(EventsSubscribeParams { subscriptions }),
        };
        let mut stream = self.transport.connect().await?;
        stream
            .write_all(encode_request(&request)?.as_bytes())
            .await
            .context("writing subscribe request")?;
        let mut lines = BufReader::new(stream).lines();
        let ack = tokio::time::timeout(REQUEST_TIMEOUT, lines.next_line())
            .await
            .map_err(|_| anyhow!("subscribe ack timed out"))?
            .context("reading subscribe ack")?
            .ok_or_else(|| anyhow!("herdr closed the connection before acking the subscription"))?;
        match expect_result(&request.id, &ack)? {
            ResponseResult::SubscriptionStarted {} => {
                debug!(session = %self.name, panes = pane_ids.len(), "subscribed");
                Ok(lines)
            }
            other => bail!("events.subscribe returned {other:?}"),
        }
    }
}

/// One request/response exchange on a fresh connection to `transport`.
/// Herdr closes the connection after replying, so nothing is reused.
pub async fn request_once(
    transport: &Transport,
    id: String,
    method: Method,
) -> Result<ResponseResult> {
    let request = Request { id, method };
    let fut = async {
        let mut stream = transport.connect().await?;
        stream
            .write_all(encode_request(&request)?.as_bytes())
            .await
            .context("writing request")?;
        let mut lines = BufReader::new(stream).lines();
        let line = lines
            .next_line()
            .await
            .context("reading response")?
            .ok_or_else(|| anyhow!("herdr closed the connection without a response"))?;
        expect_result(&request.id, &line)
    };
    tokio::time::timeout(REQUEST_TIMEOUT, fut)
        .await
        .map_err(|_| {
            anyhow!(
                "request {} timed out after {:?}",
                request.id,
                REQUEST_TIMEOUT
            )
        })?
}

fn expect_result(id: &str, line: &str) -> Result<ResponseResult> {
    match parse_frame(line).with_context(|| format!("parsing response to {id}"))? {
        Frame::Response(response) if response.id == id => Ok(response.result),
        Frame::Response(response) => bail!("response id mismatch: sent {id}, got {}", response.id),
        Frame::Error(err) => bail!("herdr error {}: {}", err.error.code, err.error.message),
        other => bail!("unexpected frame while waiting for {id}: {other:?}"),
    }
}

/// Map a pushed frame to a session event. `None` for frames Ops ignores.
pub fn dispatch(frame: Frame) -> Option<SessionEvent> {
    match frame {
        Frame::Subscription(envelope) => match envelope.data {
            SubscriptionEventData::PaneAgentStatusChanged(event) => {
                Some(SessionEvent::StatusChanged(event))
            }
            _ => None,
        },
        Frame::Broadcast(envelope) => match envelope.data {
            EventData::PaneCreated { pane } => Some(SessionEvent::PaneCreated(Box::new(pane))),
            EventData::PaneClosed { pane_id, .. } => Some(SessionEvent::PaneClosed { pane_id }),
            EventData::PaneAgentDetected {
                pane_id,
                agent,
                released,
                ..
            } => Some(SessionEvent::AgentDetected {
                pane_id,
                agent,
                released,
            }),
            EventData::PaneFocused { pane_id, .. } => Some(SessionEvent::PaneFocused { pane_id }),
            EventData::PaneUpdated { pane } => Some(SessionEvent::PaneRefreshed(Box::new(pane))),
            EventData::WorkspaceCreated { .. }
            | EventData::WorkspaceClosed { .. }
            | EventData::WorkspaceRenamed { .. }
            | EventData::WorkspaceFocused { .. } => Some(SessionEvent::WorkspacesChanged),
            _ => None,
        },
        Frame::Error(err) => {
            // Herdr closes the connection after an error on the stream
            // (`events_lost` when a subscriber falls behind its 512-event
            // history); the connection loop reconnects and re-snapshots.
            warn!(
                code = %err.error.code,
                message = %err.error.message,
                "herdr error on the event stream"
            );
            None
        }
        Frame::Response(_) | Frame::Unknown(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AgentState;
    use herdr_compat::api::schema::AgentStatus;
    use serde_json::{json, Value};
    use std::path::PathBuf;
    use tokio::net::UnixListener;

    const STATUS_FLOW: &str =
        include_str!("../../../../../../docs/fixtures/herdr-status-flow.json");

    fn fixture_lines(tag: &str) -> Vec<String> {
        let entries: Vec<Value> = serde_json::from_str(STATUS_FLOW).unwrap();
        entries
            .iter()
            .filter(|e| e["tag"] == tag)
            .filter_map(|e| e["frame"].as_str())
            .filter(|l| *l != "<EOF>" && !l.contains("\"id\""))
            .map(str::to_string)
            .collect()
    }

    fn pane_created_line() -> String {
        fixture_lines("C-broad")
            .into_iter()
            .find(|l| l.contains("pane_created"))
            .unwrap()
    }

    fn pane_json() -> Value {
        let v: Value = serde_json::from_str(&pane_created_line()).unwrap();
        v["data"]["pane"].clone()
    }

    fn temp_socket(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("ops-c-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.sock"));
        let _ = std::fs::remove_file(&path);
        path
    }

    /// A fake Herdr: one request per connection, replays the recorded
    /// status frames after a subscribe, then closes. Returns the subscribe
    /// requests it saw so tests can check pane filters.
    async fn fake_herdr(
        path: PathBuf,
        protocol: u32,
        push_after_subscribe: Vec<String>,
        seen_subscribes: mpsc::UnboundedSender<Value>,
    ) {
        let listener = UnixListener::bind(&path).unwrap();
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let mut lines = BufReader::new(stream).lines();
            let line = match lines.next_line().await.unwrap() {
                Some(l) => l,
                None => continue,
            };
            let req: Value = serde_json::from_str(&line).unwrap();
            let id = req["id"].as_str().unwrap();
            let mut stream = lines.into_inner().into_inner();
            let reply = match req["method"].as_str().unwrap() {
                "ping" => {
                    json!({"id": id, "result": {"type":"pong","version":"9.9.9","protocol":protocol,"capabilities":{"live_handoff":true}}})
                }
                "session.snapshot" => {
                    json!({"id": id, "result": {"type":"session_snapshot","snapshot":{
                        "version":"9.9.9","protocol":protocol,"tabs":[],"layouts":[],
                        "workspaces":[{"workspace_id":"w9","number":1,"label":"lcars","focused":true,"pane_count":1,"tab_count":1,"active_tab_id":"w9:t1","agent_status":"idle"}],
                        "panes":[pane_json()],
                        "agents":[]
                    }}})
                }
                "events.subscribe" => {
                    seen_subscribes.send(req.clone()).unwrap();
                    json!({"id": id, "result": {"type":"subscription_started"}})
                }
                "pane.get" => json!({"id": id, "result": {"type":"pane_info","pane": pane_json()}}),
                "agent.read" => json!({"id": id, "result": {"type":"pane_read","read": {
                    "pane_id":"w9:p2","workspace_id":"w9","tab_id":"w9:t1","source":"visible","format":"text",
                    "text":"☐ Colour\n\nWhich colour do you prefer?\n\n❯ 1. Red\n     Red\n","revision":3,"truncated":false
                }}}),
                "agent.get" => json!({"id": id, "result": {"type":"agent_info","agent": {
                    "agent":"lcars-test","agent_status":"idle","completion_seq":7,"state_change_seq":7,
                    "focused":false,"pane_id":"w9:p2","revision":1,"tab_id":"w9:t1","terminal_id":"t","workspace_id":"w9"
                }}}),
                other => panic!("fake herdr got {other}"),
            };
            stream
                .write_all(format!("{reply}\n").as_bytes())
                .await
                .unwrap();
            if req["method"] == "events.subscribe" {
                for push in &push_after_subscribe {
                    stream
                        .write_all(format!("{push}\n").as_bytes())
                        .await
                        .unwrap();
                }
                // Hold the stream open briefly, then close to end the connection.
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            drop(stream);
        }
    }

    #[tokio::test]
    async fn client_replays_recorded_flow_into_host_events() {
        let path = temp_socket("replay");
        let (subs_tx, mut subs_rx) = mpsc::unbounded_channel();
        let server = tokio::spawn(fake_herdr(
            path.clone(),
            HERDR_PROTOCOL,
            fixture_lines("A-filtered-status"),
            subs_tx,
        ));
        let (tx, mut rx) = mpsc::channel(64);
        let client = SessionClient::new(
            "fake",
            Transport::Local {
                socket: path.clone(),
            },
        );
        let runner = tokio::spawn(client.run(tx));

        let mut events = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !matches!(events.last(), Some(SessionEvent::Disconnected { .. })) {
            let (session, event) = tokio::time::timeout_at(deadline, rx.recv())
                .await
                .expect("events arrive in time")
                .expect("sender alive");
            assert_eq!(session, "fake");
            events.push(event);
        }

        assert_eq!(
            events[0],
            SessionEvent::Connected {
                version: "9.9.9".into(),
                protocol: HERDR_PROTOCOL
            }
        );
        match &events[1] {
            SessionEvent::Snapshot { panes, agents } => {
                assert_eq!(panes.len(), 1);
                assert_eq!(panes[0].pane_id, "w9:p2");
                assert!(agents.is_empty());
            }
            other => panic!("expected snapshot, got {other:?}"),
        }
        let states: Vec<AgentState> = events
            .iter()
            .filter_map(|e| match e {
                SessionEvent::StatusChanged(ev) => Some(ev.agent_status.into()),
                _ => None,
            })
            .collect();
        assert_eq!(
            states,
            vec![
                AgentState::Working,
                AgentState::Unknown,
                AgentState::Blocked,
                AgentState::Unknown,
                AgentState::Idle,
                AgentState::Unknown
            ]
        );
        // The idle event triggered an agent.get on its own task; its result
        // may land just after the stream loss, so give it a moment.
        let mut refreshed = events
            .iter()
            .any(|e| matches!(e, SessionEvent::AgentRefreshed(a) if a.completion_seq == Some(7)));
        while !refreshed {
            match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
                Ok(Some((_, SessionEvent::AgentRefreshed(a)))) if a.completion_seq == Some(7) => {
                    refreshed = true
                }
                Ok(Some(_)) => continue,
                _ => break,
            }
        }
        assert!(refreshed, "{events:?}");
        // The blocked event triggered an agent.read; its question arrives as PromptRead.
        let wanted = |e: &SessionEvent| matches!(e, SessionEvent::PromptRead { prompt: Some(p), .. } if p == "Which colour do you prefer?");
        let mut prompted = events.iter().any(wanted);
        while !prompted {
            match tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
                Ok(Some((_, e))) => prompted = wanted(&e),
                _ => break,
            }
        }
        assert!(prompted, "no PromptRead with the recorded question");
        runner.abort();
        server.abort();
        // Server closed the stream: client reports the loss and will retry.
        assert!(
            matches!(events.last(), Some(SessionEvent::Disconnected { error }) if error.contains("closed"))
        );

        let subscribe = subs_rx.recv().await.unwrap();
        let subs = subscribe["params"]["subscriptions"].as_array().unwrap();
        let kinds: Vec<&str> = subs.iter().map(|s| s["type"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            vec![
                "pane.created",
                "pane.closed",
                "pane.agent_detected",
                "pane.focused",
                "pane.updated",
                "workspace.created",
                "workspace.closed",
                "workspace.renamed",
                "workspace.focused",
                "pane.agent_status_changed"
            ]
        );
        assert_eq!(subs[9]["pane_id"], "w9:p2");
        assert!(
            subs[9].get("agent_status").is_none(),
            "no status filter: {subs:?}"
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn new_pane_triggers_a_resubscribe_with_the_added_pane() {
        let path = temp_socket("resub");
        let (subs_tx, mut subs_rx) = mpsc::unbounded_channel();
        let mut created: Value = serde_json::from_str(&pane_created_line()).unwrap();
        created["data"]["pane"]["pane_id"] = json!("w9:p7");
        let server = tokio::spawn(fake_herdr(
            path.clone(),
            HERDR_PROTOCOL,
            vec![created.to_string()],
            subs_tx,
        ));
        let (tx, mut rx) = mpsc::channel(64);
        let runner = tokio::spawn(
            SessionClient::new(
                "fake",
                Transport::Local {
                    socket: path.clone(),
                },
            )
            .run(tx),
        );

        let mut saw_created = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while !saw_created {
            let (_, event) = tokio::time::timeout_at(deadline, rx.recv())
                .await
                .unwrap()
                .unwrap();
            if let SessionEvent::PaneCreated(pane) = event {
                assert_eq!(pane.pane_id, "w9:p7");
                saw_created = true;
            }
        }
        let first = tokio::time::timeout_at(deadline, subs_rx.recv())
            .await
            .unwrap()
            .unwrap();
        let second = tokio::time::timeout_at(deadline, subs_rx.recv())
            .await
            .unwrap()
            .unwrap();
        // Frames buffered on the dropped connection are gone: a snapshot follows the swap.
        let mut snapshots_after_created = 0;
        while let Ok(Some((_, event))) =
            tokio::time::timeout(Duration::from_millis(800), rx.recv()).await
        {
            if matches!(event, SessionEvent::Snapshot { .. }) {
                snapshots_after_created += 1;
                break;
            }
        }
        assert_eq!(
            snapshots_after_created, 1,
            "expected a snapshot after the re-subscribe"
        );
        runner.abort();
        server.abort();
        let ids = |v: &Value| -> Vec<String> {
            v["params"]["subscriptions"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|s| s.get("pane_id").and_then(Value::as_str).map(String::from))
                .collect()
        };
        assert_eq!(ids(&first), vec!["w9:p2"]);
        assert_eq!(ids(&second), vec!["w9:p2", "w9:p7"]);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn protocol_mismatch_is_reported_not_connected() {
        let path = temp_socket("proto");
        let (subs_tx, _subs_rx) = mpsc::unbounded_channel();
        let server = tokio::spawn(fake_herdr(path.clone(), 21, vec![], subs_tx));
        let (tx, mut rx) = mpsc::channel(8);
        let runner = tokio::spawn(
            SessionClient::new(
                "old",
                Transport::Local {
                    socket: path.clone(),
                },
            )
            .run(tx),
        );
        let (_, event) = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .unwrap()
            .unwrap();
        runner.abort();
        server.abort();
        match event {
            SessionEvent::Disconnected { error } => {
                assert!(error.contains("protocol 21"), "{error}");
                assert!(error.contains("supports 22"), "{error}");
            }
            other => panic!("expected disconnect, got {other:?}"),
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn dispatch_maps_status_and_pane_lifecycle_and_ignores_the_rest() {
        let status = parse_frame(r#"{"data":{"agent":"x","agent_status":"blocked","pane_id":"w1:p1","workspace_id":"w1"},"event":"pane.agent_status_changed"}"#).unwrap();
        match dispatch(status) {
            Some(SessionEvent::StatusChanged(e)) => {
                assert_eq!(e.agent_status, AgentStatus::Blocked)
            }
            other => panic!("{other:?}"),
        }
        let closed = parse_frame(r#"{"data":{"pane_id":"w1:p1","type":"pane_closed","workspace_id":"w1"},"event":"pane_closed"}"#).unwrap();
        assert_eq!(
            dispatch(closed),
            Some(SessionEvent::PaneClosed {
                pane_id: "w1:p1".into()
            })
        );
        let detected = parse_frame(r#"{"data":{"agent":"x","pane_id":"w1:p1","type":"pane_agent_detected","workspace_id":"w1"},"event":"pane_agent_detected"}"#).unwrap();
        assert_eq!(
            dispatch(detected),
            Some(SessionEvent::AgentDetected {
                pane_id: "w1:p1".into(),
                agent: Some("x".into()),
                released: false
            })
        );
        let released = parse_frame(r#"{"data":{"agent":"x","final_status":"idle","pane_id":"w1:p1","released":true,"type":"pane_agent_detected","workspace_id":"w1"},"event":"pane_agent_detected"}"#).unwrap();
        assert!(matches!(
            dispatch(released),
            Some(SessionEvent::AgentDetected { released: true, .. })
        ));
        let updated = parse_frame(r#"{"data":{"type":"pane_updated","pane":{"pane_id":"w1:p1","terminal_id":"t","workspace_id":"w1","tab_id":"w1:t1","focused":false,"agent_status":"working","revision":9,"terminal_title_stripped":"New title"}},"event":"pane_updated"}"#).unwrap();
        assert!(
            matches!(dispatch(updated), Some(SessionEvent::PaneRefreshed(p)) if p.terminal_title_stripped.as_deref() == Some("New title"))
        );
        let renamed = parse_frame(r#"{"data":{"type":"workspace_renamed","workspace_id":"w1","label":"new"},"event":"workspace_renamed"}"#).unwrap();
        assert_eq!(dispatch(renamed), Some(SessionEvent::WorkspacesChanged));
        let focused = parse_frame(r#"{"data":{"pane_id":"w1:p2","type":"pane_focused","workspace_id":"w1"},"event":"pane_focused"}"#).unwrap();
        assert_eq!(
            dispatch(focused),
            Some(SessionEvent::PaneFocused {
                pane_id: "w1:p2".into()
            })
        );
        let unknown = parse_frame(r#"{"data":{},"event":"pane_teleported"}"#).unwrap();
        assert_eq!(dispatch(unknown), None);
    }
}
