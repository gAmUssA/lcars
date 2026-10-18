//! The normalised session model Ops serves to the plugin, and the merge
//! logic that folds per-host Herdr events into it.

use std::collections::BTreeMap;

use herdr_compat::api::schema::{AgentInfo, AgentStatus, PaneAgentStatusChangedEvent, PaneInfo};
use serde::{Deserialize, Serialize};

/// Agent lifecycle state as the plugin sees it. Mirrors Herdr's
/// `AgentStatus`; kept separate so the wire contract to the plugin does
/// not move when the vendored Herdr types do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentState {
    Working,
    Blocked,
    Done,
    Idle,
    Unknown,
}

impl From<AgentStatus> for AgentState {
    fn from(status: AgentStatus) -> Self {
        match status {
            AgentStatus::Working => AgentState::Working,
            AgentStatus::Blocked => AgentState::Blocked,
            AgentStatus::Done => AgentState::Done,
            AgentStatus::Idle => AgentState::Idle,
            AgentStatus::Unknown => AgentState::Unknown,
        }
    }
}

impl AgentState {
    /// Lower sorts first. Blocked agents need the operator, so they lead.
    pub fn urgency(self) -> u8 {
        match self {
            AgentState::Blocked => 0,
            AgentState::Done => 1,
            AgentState::Working => 2,
            AgentState::Idle => 3,
            AgentState::Unknown => 4,
        }
    }
}

/// One Herdr pane on one host.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// `<host>/<pane_id>`, unique across the fleet.
    pub id: String,
    pub host: String,
    pub workspace_id: String,
    pub tab_id: String,
    pub pane_id: String,
    /// Herdr's agent kind (`claude`, `codex`, `grok`, …). `None` for a plain shell.
    pub agent: Option<String>,
    /// Live agent name assigned in Herdr, if any.
    pub name: Option<String>,
    pub state: AgentState,
    pub title: Option<String>,
    pub cwd: Option<String>,
    /// From Herdr's `tokens.model`, when the agent reports one.
    pub model: Option<String>,
    /// From Herdr's `tokens.summary`: the agent's one-line summary of its last prompt.
    pub summary: Option<String>,
    pub focused: bool,
    /// Herdr's `state_change_seq`; monotonic per host.
    pub seq: u64,
    pub updated_at_ms: u64,
}

impl Session {
    pub fn id_for(host: &str, pane_id: &str) -> String {
        format!("{host}/{pane_id}")
    }

    fn from_pane(host: &str, pane: &PaneInfo, now_ms: u64) -> Self {
        Session {
            id: Session::id_for(host, &pane.pane_id),
            host: host.to_string(),
            workspace_id: pane.workspace_id.clone(),
            tab_id: pane.tab_id.clone(),
            pane_id: pane.pane_id.clone(),
            agent: pane.agent.clone(),
            name: None,
            state: pane.agent_status.into(),
            title: pane
                .label
                .clone()
                .or_else(|| pane.terminal_title_stripped.clone())
                .or_else(|| pane.title.clone()),
            cwd: pane.foreground_cwd.clone().or_else(|| pane.cwd.clone()),
            model: pane.tokens.get("model").cloned(),
            summary: pane.tokens.get("summary").cloned(),
            focused: pane.focused,
            seq: 0,
            updated_at_ms: now_ms,
        }
    }

    fn overlay_agent(&mut self, agent: &AgentInfo) {
        self.name = agent.name.clone();
        self.seq = agent.state_change_seq;
        if self.agent.is_none() {
            self.agent = agent.agent.clone();
        }
        if self.title.is_none() {
            self.title = agent.terminal_title_stripped.clone();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostStatus {
    pub name: String,
    pub connected: bool,
    pub version: Option<String>,
    pub protocol: Option<u32>,
    /// Last connection error, cleared on reconnect.
    pub error: Option<String>,
    pub updated_at_ms: u64,
}

/// What one host's Herdr connection reports to the merge.
#[derive(Debug, Clone, PartialEq)]
pub enum HostEvent {
    Connected {
        version: String,
        protocol: u32,
    },
    Disconnected {
        error: String,
    },
    /// Full state after connect. Replaces every session for the host.
    Snapshot {
        panes: Vec<PaneInfo>,
        agents: Vec<AgentInfo>,
    },
    StatusChanged(PaneAgentStatusChangedEvent),
    PaneCreated(Box<PaneInfo>),
    PaneClosed {
        pane_id: String,
    },
    /// Herdr recognised (or released) an agent in the pane.
    AgentDetected {
        pane_id: String,
        agent: Option<String>,
        released: bool,
    },
    /// Fresh `pane.get` after a detection: title, cwd, model arrive here.
    PaneRefreshed(Box<PaneInfo>),
    /// Fresh `agent.get` after a turn ended: carries `completion_seq`, from
    /// which `done` is derived for panes nobody is looking at.
    AgentRefreshed(Box<AgentInfo>),
    /// The UI focus moved to this pane; a `done` there becomes `idle`.
    PaneFocused {
        pane_id: String,
    },
}

/// Herdr's event stream never says `done`; after a turn `agent.get` shows
/// `completion_seq == state_change_seq`. An unfocused pane in that state is
/// finished work nobody has looked at yet.
pub fn derive_state(agent: &AgentInfo) -> AgentState {
    let finished = agent.agent_status == AgentStatus::Idle
        && agent.completion_seq == Some(agent.state_change_seq)
        && !agent.focused;
    if finished {
        AgentState::Done
    } else {
        agent.agent_status.into()
    }
}

/// Merged state across hosts. Cheap to clone; published as a whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fleet {
    pub hosts: BTreeMap<String, HostStatus>,
    pub sessions: BTreeMap<String, Session>,
}

impl Fleet {
    pub fn with_hosts<'a>(names: impl IntoIterator<Item = &'a str>, now_ms: u64) -> Self {
        let mut fleet = Fleet::default();
        for name in names {
            fleet.hosts.insert(
                name.to_string(),
                HostStatus {
                    name: name.to_string(),
                    connected: false,
                    version: None,
                    protocol: None,
                    error: None,
                    updated_at_ms: now_ms,
                },
            );
        }
        fleet
    }

    /// Fold one host event in. Returns `true` when anything changed.
    pub fn apply(&mut self, host: &str, event: HostEvent, now_ms: u64) -> bool {
        match event {
            HostEvent::Connected { version, protocol } => {
                let status = self.host_mut(host, now_ms);
                let changed = !status.connected
                    || status.version.as_deref() != Some(&version)
                    || status.protocol != Some(protocol)
                    || status.error.is_some();
                status.connected = true;
                status.version = Some(version);
                status.protocol = Some(protocol);
                status.error = None;
                if changed {
                    status.updated_at_ms = now_ms;
                }
                changed
            }
            HostEvent::Disconnected { error } => {
                let status = self.host_mut(host, now_ms);
                let changed = status.connected || status.error.as_deref() != Some(&error);
                status.connected = false;
                status.error = Some(error);
                if changed {
                    status.updated_at_ms = now_ms;
                }
                changed
            }
            HostEvent::Snapshot { panes, agents } => {
                let before: Vec<Session> = self.sessions_for(host).cloned().collect();
                self.sessions.retain(|_, s| s.host != host);
                for pane in &panes {
                    let session = Session::from_pane(host, pane, now_ms);
                    self.sessions.insert(session.id.clone(), session);
                }
                for agent in &agents {
                    if let Some(session) = self
                        .sessions
                        .get_mut(&Session::id_for(host, &agent.pane_id))
                    {
                        session.overlay_agent(agent);
                    }
                }
                let after: Vec<Session> = self.sessions_for(host).cloned().collect();
                !same_ignoring_time(&before, &after)
            }
            HostEvent::StatusChanged(event) => {
                let id = Session::id_for(host, &event.pane_id);
                let session = self.sessions.entry(id.clone()).or_insert_with(|| Session {
                    id,
                    host: host.to_string(),
                    workspace_id: event.workspace_id.clone(),
                    tab_id: String::new(),
                    pane_id: event.pane_id.clone(),
                    agent: None,
                    name: None,
                    state: AgentState::Unknown,
                    title: None,
                    cwd: None,
                    model: None,
                    summary: None,
                    focused: false,
                    seq: 0,
                    updated_at_ms: now_ms,
                });
                let new_state: AgentState = event.agent_status.into();
                let mut changed = session.state != new_state;
                session.state = new_state;
                if event.agent.is_some() && session.agent != event.agent {
                    session.agent = event.agent.clone();
                    changed = true;
                }
                if event.title.is_some() && session.title != event.title {
                    session.title = event.title.clone();
                    changed = true;
                }
                if changed {
                    session.updated_at_ms = now_ms;
                }
                changed
            }
            HostEvent::PaneCreated(pane) => {
                let session = Session::from_pane(host, pane.as_ref(), now_ms);
                let previous = self.sessions.insert(session.id.clone(), session.clone());
                previous.as_ref() != Some(&session)
            }
            HostEvent::PaneClosed { pane_id } => self
                .sessions
                .remove(&Session::id_for(host, &pane_id))
                .is_some(),
            HostEvent::AgentDetected {
                pane_id,
                agent,
                released,
            } => {
                let Some(session) = self.sessions.get_mut(&Session::id_for(host, &pane_id)) else {
                    return false;
                };
                let next = if released {
                    None
                } else {
                    agent.or(session.agent.clone())
                };
                if session.agent == next {
                    return false;
                }
                session.agent = next;
                if released {
                    session.name = None;
                    session.state = AgentState::Unknown;
                }
                session.updated_at_ms = now_ms;
                true
            }
            HostEvent::PaneRefreshed(pane) => {
                let fresh = Session::from_pane(host, pane.as_ref(), now_ms);
                let Some(session) = self.sessions.get_mut(&fresh.id) else {
                    self.sessions.insert(fresh.id.clone(), fresh);
                    return true;
                };
                let mut merged = fresh;
                merged.name = session.name.clone();
                merged.seq = session.seq;
                // A derived `done` outlives a pane refresh; the pane view says idle.
                if session.state == AgentState::Done && merged.state == AgentState::Idle {
                    merged.state = AgentState::Done;
                }
                merged.updated_at_ms = session.updated_at_ms;
                if *session == merged {
                    return false;
                }
                merged.updated_at_ms = now_ms;
                *session = merged;
                true
            }
            HostEvent::AgentRefreshed(agent) => {
                let id = Session::id_for(host, &agent.pane_id);
                let Some(session) = self.sessions.get_mut(&id) else {
                    return false;
                };
                let before = session.clone();
                session.overlay_agent(agent.as_ref());
                session.agent = agent.agent.clone().or(session.agent.clone());
                session.state = derive_state(agent.as_ref());
                session.focused = agent.focused;
                if let Some(model) = agent.tokens.get("model") {
                    session.model = Some(model.clone());
                }
                if let Some(summary) = agent.tokens.get("summary") {
                    session.summary = Some(summary.clone());
                }
                if agent.terminal_title_stripped.is_some() {
                    session.title = agent.terminal_title_stripped.clone();
                }
                if *session == before {
                    return false;
                }
                session.updated_at_ms = now_ms;
                true
            }
            HostEvent::PaneFocused { pane_id } => {
                let mut changed = false;
                for session in self.sessions.values_mut().filter(|s| s.host == host) {
                    let focused = session.pane_id == pane_id;
                    let mut touched = false;
                    if session.focused != focused {
                        session.focused = focused;
                        touched = true;
                    }
                    if focused && session.state == AgentState::Done {
                        session.state = AgentState::Idle;
                        touched = true;
                    }
                    if touched {
                        session.updated_at_ms = now_ms;
                        changed = true;
                    }
                }
                changed
            }
        }
    }

    fn host_mut(&mut self, host: &str, now_ms: u64) -> &mut HostStatus {
        self.hosts
            .entry(host.to_string())
            .or_insert_with(|| HostStatus {
                name: host.to_string(),
                connected: false,
                version: None,
                protocol: None,
                error: None,
                updated_at_ms: now_ms,
            })
    }

    pub fn sessions_for<'a>(&'a self, host: &'a str) -> impl Iterator<Item = &'a Session> + 'a {
        self.sessions.values().filter(move |s| s.host == host)
    }

    /// Sessions in display order: blocked first, then most recently changed.
    pub fn sessions_sorted(&self) -> Vec<&Session> {
        let mut sessions: Vec<&Session> = self.sessions.values().collect();
        sessions.sort_by(|a, b| {
            a.state
                .urgency()
                .cmp(&b.state.urgency())
                .then_with(|| b.updated_at_ms.cmp(&a.updated_at_ms))
                .then_with(|| a.id.cmp(&b.id))
        });
        sessions
    }
}

fn same_ignoring_time(a: &[Session], b: &[Session]) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            let mut y = y.clone();
            y.updated_at_ms = x.updated_at_ms;
            *x == y
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::wire::{parse_frame, Frame};
    use herdr_compat::api::schema::{EventData, SubscriptionEventData};

    const STATUS_FLOW: &str = include_str!("../../../../../docs/fixtures/herdr-status-flow.json");

    /// Recorded fixture frames for one connection tag, in order.
    fn fixture_frames(tag: &str) -> Vec<Frame> {
        let entries: Vec<serde_json::Value> = serde_json::from_str(STATUS_FLOW).unwrap();
        entries
            .iter()
            .filter(|e| e["tag"] == tag)
            .filter_map(|e| e["frame"].as_str())
            .filter(|line| *line != "<EOF>")
            .map(|line| parse_frame(line).expect("fixture frame parses"))
            .collect()
    }

    fn pane_from_fixture() -> PaneInfo {
        for frame in fixture_frames("C-broad") {
            if let Frame::Broadcast(env) = frame {
                if let EventData::PaneCreated { pane } = env.data {
                    return pane;
                }
            }
        }
        panic!("fixture has no pane_created frame");
    }

    fn status_events() -> Vec<PaneAgentStatusChangedEvent> {
        fixture_frames("A-filtered-status")
            .into_iter()
            .filter_map(|f| match f {
                Frame::Subscription(env) => match env.data {
                    SubscriptionEventData::PaneAgentStatusChanged(e) => Some(e),
                    _ => None,
                },
                _ => None,
            })
            .collect()
    }

    const T0: u64 = 1_760_000_000_000;

    #[test]
    fn connect_then_disconnect_tracks_host_status() {
        let mut fleet = Fleet::with_hosts(["mini"], T0);
        assert!(!fleet.hosts["mini"].connected);
        assert!(fleet.apply(
            "mini",
            HostEvent::Connected {
                version: "0.9.0".into(),
                protocol: 22
            },
            T0 + 1
        ));
        assert!(fleet.hosts["mini"].connected);
        assert_eq!(fleet.hosts["mini"].version.as_deref(), Some("0.9.0"));
        // Same connect again is a no-op.
        assert!(!fleet.apply(
            "mini",
            HostEvent::Connected {
                version: "0.9.0".into(),
                protocol: 22
            },
            T0 + 2
        ));
        assert!(fleet.apply(
            "mini",
            HostEvent::Disconnected {
                error: "EOF".into()
            },
            T0 + 3
        ));
        assert!(!fleet.hosts["mini"].connected);
        assert_eq!(fleet.hosts["mini"].error.as_deref(), Some("EOF"));
        assert_eq!(fleet.hosts["mini"].updated_at_ms, T0 + 3);
    }

    #[test]
    fn recorded_status_flow_replays_to_the_expected_states() {
        let mut fleet = Fleet::default();
        let pane = pane_from_fixture();
        assert!(fleet.apply("studio", HostEvent::PaneCreated(Box::new(pane)), T0));
        let session = &fleet.sessions["studio/w9:p2"];
        assert_eq!(session.state, AgentState::Unknown);
        assert_eq!(session.agent, None);
        assert_eq!(session.cwd.as_deref(), Some("/Users/me/projects/lcars"));

        // Recorded sequence: working, unknown, blocked, unknown, idle, unknown.
        // The trailing `unknown` after each report is Herdr releasing the
        // synthetic test agent (see docs/herdr-api.md, fixture caveat).
        let events = status_events();
        let states: Vec<AgentState> = events.iter().map(|e| e.agent_status.into()).collect();
        assert_eq!(
            states,
            vec![
                AgentState::Working,
                AgentState::Unknown,
                AgentState::Blocked,
                AgentState::Unknown,
                AgentState::Idle,
                AgentState::Unknown,
            ]
        );
        let mut t = T0;
        for event in events {
            t += 100;
            assert!(fleet.apply("studio", HostEvent::StatusChanged(event), t));
        }
        let session = &fleet.sessions["studio/w9:p2"];
        assert_eq!(session.state, AgentState::Unknown);
        assert_eq!(session.agent.as_deref(), Some("lcars-test"));
        assert_eq!(session.updated_at_ms, T0 + 600);

        assert!(fleet.apply(
            "studio",
            HostEvent::PaneClosed {
                pane_id: "w9:p2".into()
            },
            t
        ));
        assert!(fleet.sessions.is_empty());
        // Closing again changes nothing.
        assert!(!fleet.apply(
            "studio",
            HostEvent::PaneClosed {
                pane_id: "w9:p2".into()
            },
            t
        ));
    }

    #[test]
    fn same_state_twice_is_not_a_change() {
        let mut fleet = Fleet::default();
        fleet.apply(
            "h",
            HostEvent::PaneCreated(Box::new(pane_from_fixture())),
            T0,
        );
        let blocked = status_events()
            .into_iter()
            .find(|e| e.agent_status == AgentStatus::Blocked)
            .unwrap();
        assert!(fleet.apply("h", HostEvent::StatusChanged(blocked.clone()), T0 + 1));
        assert!(!fleet.apply("h", HostEvent::StatusChanged(blocked), T0 + 2));
        assert_eq!(fleet.sessions["h/w9:p2"].updated_at_ms, T0 + 1);
    }

    #[test]
    fn status_for_unseen_pane_creates_a_placeholder() {
        let mut fleet = Fleet::default();
        let blocked = status_events()
            .into_iter()
            .find(|e| e.agent_status == AgentStatus::Blocked)
            .unwrap();
        assert!(fleet.apply("h", HostEvent::StatusChanged(blocked), T0));
        let session = &fleet.sessions["h/w9:p2"];
        assert_eq!(session.state, AgentState::Blocked);
        assert_eq!(session.workspace_id, "w9");
        assert_eq!(session.agent.as_deref(), Some("lcars-test"));
    }

    #[test]
    fn snapshot_replaces_only_that_hosts_sessions_and_overlays_agent_names() {
        let mut fleet = Fleet::default();
        let pane = pane_from_fixture();
        fleet.apply("other", HostEvent::PaneCreated(Box::new(pane.clone())), T0);

        let agent_json = serde_json::json!({
            "agent":"claude","agent_status":"working","cwd":"/Users/me/projects/lcars",
            "focused":true,"foreground_cwd":"/Users/me/projects/lcars","name":"ops-dev",
            "pane_id":"w9:p2","revision":5,"state_change_seq":4672,"tab_id":"w9:t1",
            "terminal_id":"term_65e1ea01c6a2915","terminal_title":"◑ Coding","terminal_title_stripped":"Coding",
            "tokens":{"model":"fable-5-1","summary":"[Request interrupted by user]"},"workspace_id":"w9"
        });
        let agent: AgentInfo = serde_json::from_value(agent_json).unwrap();
        let mut working_pane = pane.clone();
        working_pane.agent = Some("claude".into());
        working_pane.agent_status = AgentStatus::Working;
        working_pane
            .tokens
            .insert("model".into(), "fable-5-1".into());

        assert!(fleet.apply(
            "studio",
            HostEvent::Snapshot {
                panes: vec![working_pane.clone()],
                agents: vec![agent],
            },
            T0 + 1
        ));
        assert_eq!(fleet.sessions.len(), 2, "other host's session survives");
        let session = &fleet.sessions["studio/w9:p2"];
        assert_eq!(session.name.as_deref(), Some("ops-dev"));
        assert_eq!(session.seq, 4672);
        assert_eq!(session.model.as_deref(), Some("fable-5-1"));
        assert_eq!(session.state, AgentState::Working);

        // Identical snapshot later: no change reported even though time moved.
        let agent_again: AgentInfo = serde_json::from_value(serde_json::json!({
            "agent":"claude","agent_status":"working","focused":true,"name":"ops-dev",
            "pane_id":"w9:p2","revision":5,"state_change_seq":4672,"tab_id":"w9:t1",
            "terminal_id":"t","terminal_title_stripped":"Coding","workspace_id":"w9"
        }))
        .unwrap();
        assert!(!fleet.apply(
            "studio",
            HostEvent::Snapshot {
                panes: vec![working_pane],
                agents: vec![agent_again],
            },
            T0 + 2
        ));

        // Empty snapshot clears the host.
        assert!(fleet.apply(
            "studio",
            HostEvent::Snapshot {
                panes: vec![],
                agents: vec![]
            },
            T0 + 3
        ));
        assert_eq!(fleet.sessions.len(), 1);
        assert!(fleet.sessions.contains_key("other/w9:p2"));
    }

    #[test]
    fn sorted_puts_blocked_first_then_most_recent() {
        let mut fleet = Fleet::default();
        let pane = pane_from_fixture();
        let mut a = pane.clone();
        a.pane_id = "w1:p1".into();
        let mut b = pane.clone();
        b.pane_id = "w1:p2".into();
        let mut c = pane;
        c.pane_id = "w1:p3".into();
        fleet.apply("h", HostEvent::PaneCreated(Box::new(a)), T0 + 30);
        fleet.apply("h", HostEvent::PaneCreated(Box::new(b)), T0 + 20);
        fleet.apply("h", HostEvent::PaneCreated(Box::new(c)), T0 + 10);
        let mut blocked = status_events()
            .into_iter()
            .find(|e| e.agent_status == AgentStatus::Blocked)
            .unwrap();
        blocked.pane_id = "w1:p3".into();
        fleet.apply("h", HostEvent::StatusChanged(blocked), T0 + 5);
        let order: Vec<&str> = fleet
            .sessions_sorted()
            .iter()
            .map(|s| s.pane_id.as_str())
            .collect();
        assert_eq!(order, vec!["w1:p3", "w1:p1", "w1:p2"]);
    }

    /// AgentInfo as `agent.get` returned it right after the recorded turn in
    /// docs/fixtures/claude-turn-flow.json: idle, completion_seq == state_change_seq.
    fn finished_agent(focused: bool) -> AgentInfo {
        serde_json::from_value(serde_json::json!({
            "agent":"claude","agent_status":"idle","completion_seq":4718,"state_change_seq":4718,
            "cwd":"/Users/me/projects/lcars","focused":focused,"foreground_cwd":"/Users/me/projects/lcars",
            "interactive_ready":true,"name":"conn-test","pane_id":"w9:p2","revision":12,"tab_id":"w9:t1",
            "terminal_id":"t","terminal_title_stripped":"Run ls -la","tokens":{"model":"fable-5-1"},"workspace_id":"w9"
        }))
        .unwrap()
    }

    #[test]
    fn finished_unfocused_turn_is_done_and_focusing_clears_it() {
        let mut fleet = Fleet::default();
        fleet.apply(
            "h",
            HostEvent::PaneCreated(Box::new(pane_from_fixture())),
            T0,
        );
        assert_eq!(derive_state(&finished_agent(false)), AgentState::Done);
        assert_eq!(derive_state(&finished_agent(true)), AgentState::Idle);
        assert!(fleet.apply(
            "h",
            HostEvent::AgentRefreshed(Box::new(finished_agent(false))),
            T0 + 1
        ));
        let s = &fleet.sessions["h/w9:p2"];
        assert_eq!(s.state, AgentState::Done);
        assert_eq!(s.name.as_deref(), Some("conn-test"));
        assert_eq!(s.title.as_deref(), Some("Run ls -la"));
        assert_eq!(s.model.as_deref(), Some("fable-5-1"));
        assert_eq!(s.seq, 4718);
        assert!(!fleet.apply(
            "h",
            HostEvent::AgentRefreshed(Box::new(finished_agent(false))),
            T0 + 2
        ));
        assert!(fleet.apply(
            "h",
            HostEvent::PaneFocused {
                pane_id: "w9:p2".into()
            },
            T0 + 3
        ));
        let s = &fleet.sessions["h/w9:p2"];
        assert_eq!(s.state, AgentState::Idle);
        assert!(s.focused);
        let mut pane = pane_from_fixture();
        pane.agent_status = AgentStatus::Idle;
        pane.terminal_title_stripped = Some("Run ls -la".into());
        assert!(fleet.apply("h", HostEvent::PaneRefreshed(Box::new(pane)), T0 + 4));
        assert_eq!(fleet.sessions["h/w9:p2"].state, AgentState::Idle);
    }

    #[test]
    fn agent_detected_sets_the_agent_and_release_clears_it() {
        let mut fleet = Fleet::default();
        fleet.apply(
            "h",
            HostEvent::PaneCreated(Box::new(pane_from_fixture())),
            T0,
        );
        assert!(fleet.apply(
            "h",
            HostEvent::AgentDetected {
                pane_id: "w9:p2".into(),
                agent: Some("claude".into()),
                released: false
            },
            T0 + 1
        ));
        assert_eq!(fleet.sessions["h/w9:p2"].agent.as_deref(), Some("claude"));
        assert!(fleet.apply(
            "h",
            HostEvent::AgentDetected {
                pane_id: "w9:p2".into(),
                agent: Some("claude".into()),
                released: true
            },
            T0 + 2
        ));
        assert_eq!(fleet.sessions["h/w9:p2"].agent, None);
        assert_eq!(fleet.sessions["h/w9:p2"].state, AgentState::Unknown);
        assert!(!fleet.apply(
            "h",
            HostEvent::AgentDetected {
                pane_id: "w9:p9".into(),
                agent: None,
                released: false
            },
            T0 + 3
        ));
    }

    #[test]
    fn pane_refresh_fills_in_title_without_losing_name_or_seq() {
        let mut fleet = Fleet::default();
        fleet.apply(
            "h",
            HostEvent::PaneCreated(Box::new(pane_from_fixture())),
            T0,
        );
        fleet.apply(
            "h",
            HostEvent::AgentRefreshed(Box::new(finished_agent(true))),
            T0 + 1,
        );
        let mut pane = pane_from_fixture();
        pane.agent = Some("claude".into());
        pane.agent_status = AgentStatus::Working;
        pane.terminal_title_stripped = Some("Refactor the parser".into());
        assert!(fleet.apply(
            "h",
            HostEvent::PaneRefreshed(Box::new(pane.clone())),
            T0 + 2
        ));
        let s = &fleet.sessions["h/w9:p2"];
        assert_eq!(s.title.as_deref(), Some("Refactor the parser"));
        assert_eq!(s.state, AgentState::Working);
        assert_eq!(s.name.as_deref(), Some("conn-test"));
        assert_eq!(s.seq, 4718);
        assert!(!fleet.apply("h", HostEvent::PaneRefreshed(Box::new(pane)), T0 + 3));
    }

    #[test]
    fn agent_state_serialises_lowercase_for_the_plugin() {
        assert_eq!(
            serde_json::to_string(&AgentState::Blocked).unwrap(),
            "\"blocked\""
        );
        assert_eq!(
            serde_json::from_str::<AgentState>("\"working\"").unwrap(),
            AgentState::Working
        );
    }
}
