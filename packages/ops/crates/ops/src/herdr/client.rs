//! Per-host connection loop: ping, snapshot, subscribe, re-subscribe on
//! new panes, reconnect with backoff. Emits `HostEvent`s; never touches
//! the merged model directly.

use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use herdr_compat::api::schema::{
    EmptyParams, EventData, EventsSubscribeParams, Method, PingParams, Request, ResponseResult,
    Subscription, SubscriptionEventData,
};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines};
use tokio::net::UnixStream;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

use super::transport::Transport;
use super::wire::{encode_request, parse_frame, Frame};
use crate::model::HostEvent;
use crate::HERDR_PROTOCOL;

/// Reconnect backoff: first retry after `INITIAL`, doubling to `MAX`.
pub const BACKOFF_INITIAL: Duration = Duration::from_secs(1);
pub const BACKOFF_MAX: Duration = Duration::from_secs(30);
/// How long one request may take before the host counts as down.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// Events from one host, tagged with its name.
pub type HostEventSender = mpsc::Sender<(String, HostEvent)>;

pub struct HostClient {
    pub name: String,
    pub transport: Transport,
    request_seq: u64,
}

impl HostClient {
    pub fn new(name: impl Into<String>, transport: Transport) -> Self {
        HostClient {
            name: name.into(),
            transport,
            request_seq: 0,
        }
    }

    /// Run forever: connect, stream events, reconnect with backoff.
    /// Ends only when the receiver is dropped.
    pub async fn run(mut self, tx: HostEventSender) {
        let mut backoff = BACKOFF_INITIAL;
        loop {
            match self.session(&tx).await {
                Ok(()) => {
                    backoff = BACKOFF_INITIAL;
                }
                Err(err) => {
                    let message = format!("{err:#}");
                    warn!(host = %self.name, error = %message, "herdr connection lost");
                    if tx
                        .send((
                            self.name.clone(),
                            HostEvent::Disconnected { error: message },
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
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
    }

    /// One connected session. Returns `Ok` only if the receiver went away;
    /// any loss of the event stream is an `Err` so the caller backs off.
    async fn session(&mut self, tx: &HostEventSender) -> Result<()> {
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
        info!(host = %self.name, version = %version, protocol, via = %self.transport.describe(), "connected to herdr");
        if tx
            .send((
                self.name.clone(),
                HostEvent::Connected { version, protocol },
            ))
            .await
            .is_err()
        {
            return Ok(());
        }

        let snapshot = match self
            .request(Method::SessionSnapshot(EmptyParams::default()))
            .await?
        {
            ResponseResult::SessionSnapshot { snapshot } => snapshot,
            other => bail!("session.snapshot returned {other:?}"),
        };
        let mut pane_ids: Vec<String> = snapshot.panes.iter().map(|p| p.pane_id.clone()).collect();
        if tx
            .send((
                self.name.clone(),
                HostEvent::Snapshot {
                    panes: snapshot.panes,
                    agents: snapshot.agents,
                },
            ))
            .await
            .is_err()
        {
            return Ok(());
        }

        let mut lines = self.subscribe(&pane_ids).await?;
        loop {
            let line = match lines
                .next_line()
                .await
                .context("reading herdr event stream")?
            {
                Some(line) => line,
                None => bail!("herdr closed the event stream"),
            };
            let frame = match parse_frame(&line) {
                Ok(frame) => frame,
                Err(err) => {
                    warn!(host = %self.name, %err, line = %line, "unparseable frame, skipping");
                    continue;
                }
            };
            let Some(event) = dispatch(frame) else {
                continue;
            };

            // A new pane needs its own status subscription; Herdr has no
            // "add subscription" call, so open a replacement connection with
            // the full set and drop the old one once the new ack arrives.
            if let HostEvent::PaneCreated(pane) = &event {
                if !pane_ids.contains(&pane.pane_id) {
                    pane_ids.push(pane.pane_id.clone());
                    match self.subscribe(&pane_ids).await {
                        Ok(fresh) => lines = fresh,
                        Err(err) => {
                            warn!(host = %self.name, pane = %pane.pane_id, %err, "could not re-subscribe for new pane; keeping old stream");
                            pane_ids.pop();
                        }
                    }
                }
            }
            if let HostEvent::PaneClosed { pane_id } = &event {
                pane_ids.retain(|id| id != pane_id);
            }
            if tx.send((self.name.clone(), event)).await.is_err() {
                return Ok(());
            }
        }
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
        let mut subscriptions = vec![Subscription::PaneCreated {}, Subscription::PaneClosed {}];
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
                debug!(host = %self.name, panes = pane_ids.len(), "subscribed");
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

/// Map a pushed frame to a host event. `None` for frames Ops ignores.
pub fn dispatch(frame: Frame) -> Option<HostEvent> {
    match frame {
        Frame::Subscription(envelope) => match envelope.data {
            SubscriptionEventData::PaneAgentStatusChanged(event) => {
                Some(HostEvent::StatusChanged(event))
            }
            _ => None,
        },
        Frame::Broadcast(envelope) => match envelope.data {
            EventData::PaneCreated { pane } => Some(HostEvent::PaneCreated(Box::new(pane))),
            EventData::PaneClosed { pane_id, .. } => Some(HostEvent::PaneClosed { pane_id }),
            _ => None,
        },
        Frame::Response(_) | Frame::Error(_) | Frame::Unknown(_) => None,
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
                        "version":"9.9.9","protocol":protocol,"workspaces":[],"tabs":[],"layouts":[],
                        "panes":[pane_json()],
                        "agents":[]
                    }}})
                }
                "events.subscribe" => {
                    seen_subscribes.send(req.clone()).unwrap();
                    json!({"id": id, "result": {"type":"subscription_started"}})
                }
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
                // Hold the stream open briefly, then close to end the session.
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
        let client = HostClient::new(
            "fake",
            Transport::Local {
                socket: path.clone(),
            },
        );
        let runner = tokio::spawn(client.run(tx));

        let mut events = Vec::new();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while events.len() < 9 {
            let (host, event) = tokio::time::timeout_at(deadline, rx.recv())
                .await
                .expect("events arrive in time")
                .expect("sender alive");
            assert_eq!(host, "fake");
            events.push(event);
        }
        runner.abort();
        server.abort();

        assert_eq!(
            events[0],
            HostEvent::Connected {
                version: "9.9.9".into(),
                protocol: HERDR_PROTOCOL
            }
        );
        match &events[1] {
            HostEvent::Snapshot { panes, agents } => {
                assert_eq!(panes.len(), 1);
                assert_eq!(panes[0].pane_id, "w9:p2");
                assert!(agents.is_empty());
            }
            other => panic!("expected snapshot, got {other:?}"),
        }
        let states: Vec<AgentState> = events[2..8]
            .iter()
            .map(|e| match e {
                HostEvent::StatusChanged(ev) => ev.agent_status.into(),
                other => panic!("expected status change, got {other:?}"),
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
        // Server closed the stream: client reports the loss and will retry.
        assert!(
            matches!(&events[8], HostEvent::Disconnected { error } if error.contains("closed"))
        );

        let subscribe = subs_rx.recv().await.unwrap();
        let subs = subscribe["params"]["subscriptions"].as_array().unwrap();
        let kinds: Vec<&str> = subs.iter().map(|s| s["type"].as_str().unwrap()).collect();
        assert_eq!(
            kinds,
            vec!["pane.created", "pane.closed", "pane.agent_status_changed"]
        );
        assert_eq!(subs[2]["pane_id"], "w9:p2");
        assert!(
            subs[2].get("agent_status").is_none(),
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
            HostClient::new(
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
            if let HostEvent::PaneCreated(pane) = event {
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
            HostClient::new(
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
            HostEvent::Disconnected { error } => {
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
            Some(HostEvent::StatusChanged(e)) => assert_eq!(e.agent_status, AgentStatus::Blocked),
            other => panic!("{other:?}"),
        }
        let closed = parse_frame(r#"{"data":{"pane_id":"w1:p1","type":"pane_closed","workspace_id":"w1"},"event":"pane_closed"}"#).unwrap();
        assert_eq!(
            dispatch(closed),
            Some(HostEvent::PaneClosed {
                pane_id: "w1:p1".into()
            })
        );
        let detected = parse_frame(r#"{"data":{"agent":"x","pane_id":"w1:p1","type":"pane_agent_detected","workspace_id":"w1"},"event":"pane_agent_detected"}"#).unwrap();
        assert_eq!(dispatch(detected), None);
        let unknown = parse_frame(r#"{"data":{},"event":"pane_teleported"}"#).unwrap();
        assert_eq!(dispatch(unknown), None);
    }
}
