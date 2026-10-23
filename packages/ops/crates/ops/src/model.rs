//! The normalised pane model Ops serves to the plugin, and the merge
//! logic that folds per-session Herdr events into it.

use std::collections::BTreeMap;

use herdr_compat::api::schema::{
    AgentInfo, AgentStatus, PaneAgentStatusChangedEvent, PaneInfo, WorkspaceInfo,
};
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

/// One Herdr pane on one session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pane {
    /// `<session>/<pane_id>`, unique across the fleet.
    pub id: String,
    pub session: String,
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
    /// Herdr's `state_change_seq`; monotonic per session.
    pub seq: u64,
    pub updated_at_ms: u64,
    /// The question a blocked agent is asking, read from its screen; `None`
    /// unless the pane is blocked and the dialog text was recognised.
    #[serde(default)]
    pub pending_prompt: Option<String>,
}

/// Pull the question out of a visible-screen read of a blocked agent.
///
/// Observed dialogs (docs/fixtures/claude-*.json): Claude's option menus put
/// the question on its own line above `❯ 1. …`; the folder-trust screen ends
/// its question with `?` too. Take the last line ending in `?` above the
/// first option marker, else the last non-empty line above it.
pub fn extract_prompt(screen: &str) -> Option<String> {
    let lines: Vec<&str> = screen.lines().map(str::trim_end).collect();
    let marker = lines
        .iter()
        .position(|l| l.trim_start().starts_with('❯') || l.trim_start().starts_with("> "));
    let above: &[&str] = match marker {
        Some(i) => &lines[..i],
        None => &lines,
    };
    let candidates: Vec<&str> = above
        .iter()
        .map(|l| l.trim())
        .filter(|l| {
            !l.is_empty()
                && !l.starts_with('─')
                && !l.starts_with('☐')
                && !l.starts_with('╭')
                && !l.starts_with('│')
        })
        .collect();
    let question = candidates.iter().rev().find(|l| l.ends_with('?'));
    let pick = question.or(candidates.last())?;
    let text: String = pick.chars().take(160).collect();
    (!text.is_empty()).then_some(text)
}

impl Pane {
    pub fn id_for(session: &str, pane_id: &str) -> String {
        format!("{session}/{pane_id}")
    }

    fn from_pane(session: &str, pane: &PaneInfo, now_ms: u64) -> Self {
        Pane {
            id: Pane::id_for(session, &pane.pane_id),
            session: session.to_string(),
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
            pending_prompt: None,
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

/// One Herdr workspace on one session, for the workspace keys.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// `<session>/<workspace_id>`.
    pub id: String,
    pub session: String,
    pub workspace_id: String,
    /// Herdr's 1-based ordinal, the order the TUI shows.
    pub number: usize,
    pub label: String,
    pub focused: bool,
    pub pane_count: usize,
    /// Herdr's own roll-up of the agents inside.
    pub state: AgentState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStatus {
    pub name: String,
    /// SSH alias of the machine the session runs on; `None` on this machine.
    #[serde(default)]
    pub machine: Option<String>,
    /// True for the session whose Herdr socket is on this machine: the deck's
    /// own session, which pane-level dials prefer.
    pub local: bool,
    pub connected: bool,
    pub version: Option<String>,
    pub protocol: Option<u32>,
    /// Last connection error, cleared on reconnect.
    pub error: Option<String>,
    pub updated_at_ms: u64,
}

/// What one session's Herdr connection reports to the merge.
#[derive(Debug, Clone, PartialEq)]
pub enum SessionEvent {
    Connected {
        version: String,
        protocol: u32,
    },
    Disconnected {
        error: String,
    },
    /// Full state after connect. Replaces every pane for the session.
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
    /// The question on a blocked pane's screen, read after it blocked.
    PromptRead {
        pane_id: String,
        prompt: Option<String>,
    },
    /// The session's workspace list, from a snapshot. Replaces the session's set.
    Workspaces(Vec<WorkspaceInfo>),
    /// A workspace was created, closed, renamed or focused; the client
    /// answers with a fresh snapshot, the merge needs nothing.
    WorkspacesChanged,
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

/// Merged state across sessions. Cheap to clone; published as a whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fleet {
    pub sessions: BTreeMap<String, SessionStatus>,
    pub panes: BTreeMap<String, Pane>,
    #[serde(default)]
    pub workspaces: BTreeMap<String, Workspace>,
}

impl Fleet {
    /// One entry per configured session: its name and, for a remote
    /// session, the machine it runs on.
    pub fn with_sessions<'a>(
        sessions: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
        now_ms: u64,
    ) -> Self {
        let mut fleet = Fleet::default();
        for (name, machine) in sessions {
            let local = machine.is_none();
            let machine = machine.map(str::to_string);
            fleet.sessions.insert(
                name.to_string(),
                SessionStatus {
                    name: name.to_string(),
                    machine,
                    local,
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

    /// Fold one session event in. Returns `true` when anything changed.
    pub fn apply(&mut self, session: &str, event: SessionEvent, now_ms: u64) -> bool {
        match event {
            SessionEvent::Connected { version, protocol } => {
                let status = self.session_mut(session, now_ms);
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
            SessionEvent::Disconnected { error } => {
                let status = self.session_mut(session, now_ms);
                let changed = status.connected || status.error.as_deref() != Some(&error);
                status.connected = false;
                status.error = Some(error);
                if changed {
                    status.updated_at_ms = now_ms;
                }
                changed
            }
            SessionEvent::Snapshot { panes, agents } => {
                let before: Vec<Pane> = self.panes_for(session).cloned().collect();
                self.panes.retain(|_, s| s.session != session);
                for pane in &panes {
                    let pane = Pane::from_pane(session, pane, now_ms);
                    self.panes.insert(pane.id.clone(), pane);
                }
                for agent in &agents {
                    if let Some(pane) = self.panes.get_mut(&Pane::id_for(session, &agent.pane_id)) {
                        pane.overlay_agent(agent);
                        pane.state = derive_state(agent);
                        pane.focused = agent.focused;
                    }
                }
                let after: Vec<Pane> = self.panes_for(session).cloned().collect();
                !same_ignoring_time(&before, &after)
            }
            SessionEvent::StatusChanged(event) => {
                let id = Pane::id_for(session, &event.pane_id);
                let pane = self.panes.entry(id.clone()).or_insert_with(|| Pane {
                    id,
                    session: session.to_string(),
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
                    pending_prompt: None,
                });
                let new_state: AgentState = event.agent_status.into();
                // A finished turn stays `done` through a repeated idle event;
                // only focus or a fresh agent.get decides otherwise.
                let keep_done = pane.state == AgentState::Done && new_state == AgentState::Idle;
                let mut changed = !keep_done && pane.state != new_state;
                if !keep_done {
                    pane.state = new_state;
                }
                if new_state != AgentState::Blocked && pane.pending_prompt.is_some() {
                    pane.pending_prompt = None;
                    changed = true;
                }
                if event.agent.is_some() && pane.agent != event.agent {
                    pane.agent = event.agent.clone();
                    changed = true;
                }
                if event.title.is_some() && pane.title != event.title {
                    pane.title = event.title.clone();
                    changed = true;
                }
                if changed {
                    pane.updated_at_ms = now_ms;
                }
                changed
            }
            SessionEvent::PaneCreated(pane) => {
                let pane = Pane::from_pane(session, pane.as_ref(), now_ms);
                let previous = self.panes.insert(pane.id.clone(), pane.clone());
                previous.as_ref() != Some(&pane)
            }
            SessionEvent::PaneClosed { pane_id } => self
                .panes
                .remove(&Pane::id_for(session, &pane_id))
                .is_some(),
            SessionEvent::AgentDetected {
                pane_id,
                agent,
                released,
            } => {
                let Some(pane) = self.panes.get_mut(&Pane::id_for(session, &pane_id)) else {
                    return false;
                };
                let next = if released {
                    None
                } else {
                    agent.or(pane.agent.clone())
                };
                if pane.agent == next {
                    return false;
                }
                pane.agent = next;
                if released {
                    pane.name = None;
                    pane.state = AgentState::Unknown;
                }
                pane.updated_at_ms = now_ms;
                true
            }
            SessionEvent::PaneRefreshed(pane) => {
                let fresh = Pane::from_pane(session, pane.as_ref(), now_ms);
                let Some(pane) = self.panes.get_mut(&fresh.id) else {
                    self.panes.insert(fresh.id.clone(), fresh);
                    return true;
                };
                let mut merged = fresh;
                merged.name = pane.name.clone();
                merged.seq = pane.seq;
                if merged.state == AgentState::Blocked {
                    merged.pending_prompt = pane.pending_prompt.clone();
                }
                // A derived `done` outlives a pane refresh; the pane view says idle.
                if pane.state == AgentState::Done && merged.state == AgentState::Idle {
                    merged.state = AgentState::Done;
                }
                merged.updated_at_ms = pane.updated_at_ms;
                if *pane == merged {
                    return false;
                }
                merged.updated_at_ms = now_ms;
                *pane = merged;
                true
            }
            SessionEvent::AgentRefreshed(agent) => {
                let id = Pane::id_for(session, &agent.pane_id);
                let mut changed = false;
                if agent.focused {
                    // One focused pane per session.
                    for other in self
                        .panes
                        .values_mut()
                        .filter(|s| s.session == session && s.id != id)
                    {
                        if other.focused {
                            other.focused = false;
                            other.updated_at_ms = now_ms;
                            changed = true;
                        }
                    }
                }
                let Some(pane) = self.panes.get_mut(&id) else {
                    return changed;
                };
                let before = pane.clone();
                pane.overlay_agent(agent.as_ref());
                pane.agent = agent.agent.clone().or(pane.agent.clone());
                pane.state = derive_state(agent.as_ref());
                pane.focused = agent.focused;
                if let Some(model) = agent.tokens.get("model") {
                    pane.model = Some(model.clone());
                }
                if let Some(summary) = agent.tokens.get("summary") {
                    pane.summary = Some(summary.clone());
                }
                if agent.terminal_title_stripped.is_some() {
                    pane.title = agent.terminal_title_stripped.clone();
                }
                if *pane == before {
                    return changed;
                }
                pane.updated_at_ms = now_ms;
                true
            }
            SessionEvent::PromptRead { pane_id, prompt } => {
                let Some(pane) = self.panes.get_mut(&Pane::id_for(session, &pane_id)) else {
                    return false;
                };
                // The read raced a state change: an unblocked pane asks nothing.
                if pane.state != AgentState::Blocked || pane.pending_prompt == prompt {
                    return false;
                }
                pane.pending_prompt = prompt;
                pane.updated_at_ms = now_ms;
                true
            }
            SessionEvent::WorkspacesChanged => false,
            SessionEvent::Workspaces(list) => {
                let before: Vec<Workspace> = self
                    .workspaces
                    .values()
                    .filter(|w| w.session == session)
                    .cloned()
                    .collect();
                self.workspaces.retain(|_, w| w.session != session);
                for info in &list {
                    let id = format!("{session}/{}", info.workspace_id);
                    self.workspaces.insert(
                        id.clone(),
                        Workspace {
                            id,
                            session: session.to_string(),
                            workspace_id: info.workspace_id.clone(),
                            number: info.number,
                            label: info.label.clone(),
                            focused: info.focused,
                            pane_count: info.pane_count,
                            state: info.agent_status.into(),
                        },
                    );
                }
                let after: Vec<Workspace> = self
                    .workspaces
                    .values()
                    .filter(|w| w.session == session)
                    .cloned()
                    .collect();
                before != after
            }
            SessionEvent::PaneFocused { pane_id } => {
                let mut changed = false;
                for pane in self.panes.values_mut().filter(|s| s.session == session) {
                    let focused = pane.pane_id == pane_id;
                    let mut touched = false;
                    if pane.focused != focused {
                        pane.focused = focused;
                        touched = true;
                    }
                    if focused && pane.state == AgentState::Done {
                        pane.state = AgentState::Idle;
                        touched = true;
                    }
                    if touched {
                        pane.updated_at_ms = now_ms;
                        changed = true;
                    }
                }
                changed
            }
        }
    }

    fn session_mut(&mut self, session: &str, now_ms: u64) -> &mut SessionStatus {
        self.sessions
            .entry(session.to_string())
            .or_insert_with(|| SessionStatus {
                name: session.to_string(),
                machine: None,
                local: false,
                connected: false,
                version: None,
                protocol: None,
                error: None,
                updated_at_ms: now_ms,
            })
    }

    pub fn panes_for<'a>(&'a self, session: &'a str) -> impl Iterator<Item = &'a Pane> + 'a {
        self.panes.values().filter(move |s| s.session == session)
    }

    /// Workspaces in display order: by session name, then Herdr's ordinal.
    pub fn workspaces_sorted(&self) -> Vec<&Workspace> {
        let mut list: Vec<&Workspace> = self.workspaces.values().collect();
        list.sort_by(|a, b| {
            a.session
                .cmp(&b.session)
                .then_with(|| a.number.cmp(&b.number))
        });
        list
    }

    /// Panes in display order: blocked first, then most recently changed.
    pub fn panes_sorted(&self) -> Vec<&Pane> {
        let mut panes: Vec<&Pane> = self.panes.values().collect();
        panes.sort_by(|a, b| {
            a.state
                .urgency()
                .cmp(&b.state.urgency())
                .then_with(|| b.updated_at_ms.cmp(&a.updated_at_ms))
                .then_with(|| a.id.cmp(&b.id))
        });
        panes
    }
}

fn same_ignoring_time(a: &[Pane], b: &[Pane]) -> bool {
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
        let mut fleet = Fleet::with_sessions([("mini", Some("mini"))], T0);
        assert!(!fleet.sessions["mini"].connected);
        assert!(fleet.apply(
            "mini",
            SessionEvent::Connected {
                version: "0.9.0".into(),
                protocol: 22
            },
            T0 + 1
        ));
        assert!(fleet.sessions["mini"].connected);
        assert_eq!(fleet.sessions["mini"].version.as_deref(), Some("0.9.0"));
        // Same connect again is a no-op.
        assert!(!fleet.apply(
            "mini",
            SessionEvent::Connected {
                version: "0.9.0".into(),
                protocol: 22
            },
            T0 + 2
        ));
        assert!(fleet.apply(
            "mini",
            SessionEvent::Disconnected {
                error: "EOF".into()
            },
            T0 + 3
        ));
        assert!(!fleet.sessions["mini"].connected);
        assert_eq!(fleet.sessions["mini"].error.as_deref(), Some("EOF"));
        assert_eq!(fleet.sessions["mini"].updated_at_ms, T0 + 3);
    }

    #[test]
    fn recorded_status_flow_replays_to_the_expected_states() {
        let mut fleet = Fleet::default();
        let pane = pane_from_fixture();
        assert!(fleet.apply("studio", SessionEvent::PaneCreated(Box::new(pane)), T0));
        let pane = &fleet.panes["studio/w9:p2"];
        assert_eq!(pane.state, AgentState::Unknown);
        assert_eq!(pane.agent, None);
        assert_eq!(pane.cwd.as_deref(), Some("/Users/me/projects/lcars"));

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
            assert!(fleet.apply("studio", SessionEvent::StatusChanged(event), t));
        }
        let pane = &fleet.panes["studio/w9:p2"];
        assert_eq!(pane.state, AgentState::Unknown);
        assert_eq!(pane.agent.as_deref(), Some("lcars-test"));
        assert_eq!(pane.updated_at_ms, T0 + 600);

        assert!(fleet.apply(
            "studio",
            SessionEvent::PaneClosed {
                pane_id: "w9:p2".into()
            },
            t
        ));
        assert!(fleet.panes.is_empty());
        // Closing again changes nothing.
        assert!(!fleet.apply(
            "studio",
            SessionEvent::PaneClosed {
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
            SessionEvent::PaneCreated(Box::new(pane_from_fixture())),
            T0,
        );
        let blocked = status_events()
            .into_iter()
            .find(|e| e.agent_status == AgentStatus::Blocked)
            .unwrap();
        assert!(fleet.apply("h", SessionEvent::StatusChanged(blocked.clone()), T0 + 1));
        assert!(!fleet.apply("h", SessionEvent::StatusChanged(blocked), T0 + 2));
        assert_eq!(fleet.panes["h/w9:p2"].updated_at_ms, T0 + 1);
    }

    #[test]
    fn status_for_unseen_pane_creates_a_placeholder() {
        let mut fleet = Fleet::default();
        let blocked = status_events()
            .into_iter()
            .find(|e| e.agent_status == AgentStatus::Blocked)
            .unwrap();
        assert!(fleet.apply("h", SessionEvent::StatusChanged(blocked), T0));
        let pane = &fleet.panes["h/w9:p2"];
        assert_eq!(pane.state, AgentState::Blocked);
        assert_eq!(pane.workspace_id, "w9");
        assert_eq!(pane.agent.as_deref(), Some("lcars-test"));
    }

    #[test]
    fn snapshot_replaces_only_that_hosts_sessions_and_overlays_agent_names() {
        let mut fleet = Fleet::default();
        let pane = pane_from_fixture();
        fleet.apply(
            "other",
            SessionEvent::PaneCreated(Box::new(pane.clone())),
            T0,
        );

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
            SessionEvent::Snapshot {
                panes: vec![working_pane.clone()],
                agents: vec![agent],
            },
            T0 + 1
        ));
        assert_eq!(fleet.panes.len(), 2, "other session's pane survives");
        let pane = &fleet.panes["studio/w9:p2"];
        assert_eq!(pane.name.as_deref(), Some("ops-dev"));
        assert_eq!(pane.seq, 4672);
        assert_eq!(pane.model.as_deref(), Some("fable-5-1"));
        assert_eq!(pane.state, AgentState::Working);

        // Identical snapshot later: no change reported even though time moved.
        let agent_again: AgentInfo = serde_json::from_value(serde_json::json!({
            "agent":"claude","agent_status":"working","focused":true,"name":"ops-dev",
            "pane_id":"w9:p2","revision":5,"state_change_seq":4672,"tab_id":"w9:t1",
            "terminal_id":"t","terminal_title_stripped":"Coding","workspace_id":"w9"
        }))
        .unwrap();
        assert!(!fleet.apply(
            "studio",
            SessionEvent::Snapshot {
                panes: vec![working_pane],
                agents: vec![agent_again],
            },
            T0 + 2
        ));

        // Empty snapshot clears the session.
        assert!(fleet.apply(
            "studio",
            SessionEvent::Snapshot {
                panes: vec![],
                agents: vec![]
            },
            T0 + 3
        ));
        assert_eq!(fleet.panes.len(), 1);
        assert!(fleet.panes.contains_key("other/w9:p2"));
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
        fleet.apply("h", SessionEvent::PaneCreated(Box::new(a)), T0 + 30);
        fleet.apply("h", SessionEvent::PaneCreated(Box::new(b)), T0 + 20);
        fleet.apply("h", SessionEvent::PaneCreated(Box::new(c)), T0 + 10);
        let mut blocked = status_events()
            .into_iter()
            .find(|e| e.agent_status == AgentStatus::Blocked)
            .unwrap();
        blocked.pane_id = "w1:p3".into();
        fleet.apply("h", SessionEvent::StatusChanged(blocked), T0 + 5);
        let order: Vec<&str> = fleet
            .panes_sorted()
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
            SessionEvent::PaneCreated(Box::new(pane_from_fixture())),
            T0,
        );
        assert_eq!(derive_state(&finished_agent(false)), AgentState::Done);
        assert_eq!(derive_state(&finished_agent(true)), AgentState::Idle);
        assert!(fleet.apply(
            "h",
            SessionEvent::AgentRefreshed(Box::new(finished_agent(false))),
            T0 + 1
        ));
        let s = &fleet.panes["h/w9:p2"];
        assert_eq!(s.state, AgentState::Done);
        assert_eq!(s.name.as_deref(), Some("conn-test"));
        assert_eq!(s.title.as_deref(), Some("Run ls -la"));
        assert_eq!(s.model.as_deref(), Some("fable-5-1"));
        assert_eq!(s.seq, 4718);
        assert!(!fleet.apply(
            "h",
            SessionEvent::AgentRefreshed(Box::new(finished_agent(false))),
            T0 + 2
        ));
        assert!(fleet.apply(
            "h",
            SessionEvent::PaneFocused {
                pane_id: "w9:p2".into()
            },
            T0 + 3
        ));
        let s = &fleet.panes["h/w9:p2"];
        assert_eq!(s.state, AgentState::Idle);
        assert!(s.focused);
        let mut pane = pane_from_fixture();
        pane.agent_status = AgentStatus::Idle;
        pane.terminal_title_stripped = Some("Run ls -la".into());
        assert!(fleet.apply("h", SessionEvent::PaneRefreshed(Box::new(pane)), T0 + 4));
        assert_eq!(fleet.panes["h/w9:p2"].state, AgentState::Idle);
    }

    #[test]
    fn snapshot_derives_done_and_a_repeated_idle_keeps_it() {
        let mut fleet = Fleet::default();
        let mut pane = pane_from_fixture();
        pane.agent = Some("claude".into());
        pane.agent_status = AgentStatus::Idle;
        assert!(fleet.apply(
            "h",
            SessionEvent::Snapshot {
                panes: vec![pane],
                agents: vec![finished_agent(false)],
            },
            T0
        ));
        assert_eq!(fleet.panes["h/w9:p2"].state, AgentState::Done);
        // A duplicate idle event (follow-up agent.get lost) does not clear done.
        let idle = PaneAgentStatusChangedEvent {
            pane_id: "w9:p2".into(),
            workspace_id: "w9".into(),
            agent_status: AgentStatus::Idle,
            agent: Some("claude".into()),
            title: None,
            display_agent: None,
            state_labels: Default::default(),
        };
        assert!(!fleet.apply("h", SessionEvent::StatusChanged(idle), T0 + 1));
        assert_eq!(fleet.panes["h/w9:p2"].state, AgentState::Done);
    }

    #[test]
    fn agent_refresh_with_focus_clears_the_hosts_other_focused_pane() {
        let mut fleet = Fleet::default();
        let mut a = pane_from_fixture();
        a.focused = true;
        let mut b = pane_from_fixture();
        b.pane_id = "w9:p3".into();
        fleet.apply("h", SessionEvent::PaneCreated(Box::new(a)), T0);
        fleet.apply("h", SessionEvent::PaneCreated(Box::new(b)), T0);
        let mut refreshed = finished_agent(true);
        refreshed.pane_id = "w9:p3".into();
        assert!(fleet.apply(
            "h",
            SessionEvent::AgentRefreshed(Box::new(refreshed)),
            T0 + 1
        ));
        assert!(!fleet.panes["h/w9:p2"].focused);
        assert!(fleet.panes["h/w9:p3"].focused);
    }

    #[test]
    fn agent_detected_sets_the_agent_and_release_clears_it() {
        let mut fleet = Fleet::default();
        fleet.apply(
            "h",
            SessionEvent::PaneCreated(Box::new(pane_from_fixture())),
            T0,
        );
        assert!(fleet.apply(
            "h",
            SessionEvent::AgentDetected {
                pane_id: "w9:p2".into(),
                agent: Some("claude".into()),
                released: false
            },
            T0 + 1
        ));
        assert_eq!(fleet.panes["h/w9:p2"].agent.as_deref(), Some("claude"));
        assert!(fleet.apply(
            "h",
            SessionEvent::AgentDetected {
                pane_id: "w9:p2".into(),
                agent: Some("claude".into()),
                released: true
            },
            T0 + 2
        ));
        assert_eq!(fleet.panes["h/w9:p2"].agent, None);
        assert_eq!(fleet.panes["h/w9:p2"].state, AgentState::Unknown);
        assert!(!fleet.apply(
            "h",
            SessionEvent::AgentDetected {
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
            SessionEvent::PaneCreated(Box::new(pane_from_fixture())),
            T0,
        );
        fleet.apply(
            "h",
            SessionEvent::AgentRefreshed(Box::new(finished_agent(true))),
            T0 + 1,
        );
        let mut pane = pane_from_fixture();
        pane.agent = Some("claude".into());
        pane.agent_status = AgentStatus::Working;
        pane.terminal_title_stripped = Some("Refactor the parser".into());
        assert!(fleet.apply(
            "h",
            SessionEvent::PaneRefreshed(Box::new(pane.clone())),
            T0 + 2
        ));
        let s = &fleet.panes["h/w9:p2"];
        assert_eq!(s.title.as_deref(), Some("Refactor the parser"));
        assert_eq!(s.state, AgentState::Working);
        assert_eq!(s.name.as_deref(), Some("conn-test"));
        assert_eq!(s.seq, 4718);
        assert!(!fleet.apply("h", SessionEvent::PaneRefreshed(Box::new(pane)), T0 + 3));
    }

    #[test]
    fn workspaces_replace_per_host_and_sort_by_host_then_number() {
        let mut fleet = Fleet::default();
        let ws = |id: &str,
                  number: usize,
                  label: &str,
                  status: &str,
                  focused: bool|
         -> WorkspaceInfo {
            serde_json::from_value(serde_json::json!({
                "workspace_id": id, "number": number, "label": label, "focused": focused,
                "pane_count": 3, "tab_count": 2, "active_tab_id": format!("{id}:t1"), "agent_status": status
            }))
            .unwrap()
        };
        assert!(fleet.apply(
            "studio",
            SessionEvent::Workspaces(vec![
                ws("w4", 2, "railway", "idle", false),
                ws("w1", 1, "dotfiles", "working", true)
            ]),
            T0
        ));
        assert!(fleet.apply(
            "mini",
            SessionEvent::Workspaces(vec![ws("w7", 1, "watch", "blocked", true)]),
            T0
        ));
        let order: Vec<&str> = fleet
            .workspaces_sorted()
            .iter()
            .map(|w| w.id.as_str())
            .collect();
        assert_eq!(order, vec!["mini/w7", "studio/w1", "studio/w4"]);
        assert_eq!(fleet.workspaces["studio/w1"].state, AgentState::Working);
        assert_eq!(fleet.workspaces["mini/w7"].state, AgentState::Blocked);
        // Same list again: no change. A shorter list drops the missing one.
        assert!(!fleet.apply(
            "studio",
            SessionEvent::Workspaces(vec![
                ws("w4", 2, "railway", "idle", false),
                ws("w1", 1, "dotfiles", "working", true)
            ]),
            T0 + 1
        ));
        assert!(fleet.apply(
            "studio",
            SessionEvent::Workspaces(vec![ws("w1", 1, "dotfiles", "idle", true)]),
            T0 + 2
        ));
        assert_eq!(fleet.workspaces.len(), 2);
        assert!(!fleet.apply("studio", SessionEvent::WorkspacesChanged, T0 + 3));
    }

    /// Screens recorded in docs/fixtures/claude-question-flow.json and
    /// claude-trust-dialog-flow.json.
    const QUESTION_SCREEN: &str = "☐ Colour\n\nWhich colour do you prefer?\n\n❯ 1. Red\n     Red\n↓ 2. Green\n     Green\n─────────────\n  5. Chat about this\n\nEnter to select · ↑/↓ to navigate · Esc to cancel\n";
    const TRUST_SCREEN: &str = "this folder first.\n\n Claude Code'll be able to read, edit, and execute\n files here.\n\n Security guide\n\n ❯ No, exit\n   Yes, I trust this folder\n\n Enter to confirm · Esc to cancel\n";

    #[test]
    fn extract_prompt_finds_the_question_above_the_options() {
        assert_eq!(
            extract_prompt(QUESTION_SCREEN).as_deref(),
            Some("Which colour do you prefer?")
        );
        // No question mark above the marker: the last line of the dialog text.
        assert_eq!(
            extract_prompt(TRUST_SCREEN).as_deref(),
            Some("Security guide")
        );
        assert_eq!(extract_prompt(""), None);
        assert_eq!(extract_prompt("\n───\n\n"), None);
        let long = format!("{}?\n❯ 1. ok\n", "x".repeat(300));
        assert_eq!(extract_prompt(&long).unwrap().chars().count(), 160);
    }

    #[test]
    fn prompt_read_sticks_to_blocked_sessions_and_clears_when_they_move_on() {
        let mut fleet = Fleet::default();
        fleet.apply(
            "h",
            SessionEvent::PaneCreated(Box::new(pane_from_fixture())),
            T0,
        );
        let blocked = status_events()
            .into_iter()
            .find(|e| e.agent_status == AgentStatus::Blocked)
            .unwrap();
        // A read that lands on a non-blocked pane is dropped.
        assert!(!fleet.apply(
            "h",
            SessionEvent::PromptRead {
                pane_id: "w9:p2".into(),
                prompt: Some("Which colour?".into())
            },
            T0 + 1
        ));
        fleet.apply("h", SessionEvent::StatusChanged(blocked), T0 + 2);
        assert!(fleet.apply(
            "h",
            SessionEvent::PromptRead {
                pane_id: "w9:p2".into(),
                prompt: Some("Which colour do you prefer?".into())
            },
            T0 + 3
        ));
        assert_eq!(
            fleet.panes["h/w9:p2"].pending_prompt.as_deref(),
            Some("Which colour do you prefer?")
        );
        // A pane refresh while still blocked keeps it.
        let mut pane = pane_from_fixture();
        pane.agent_status = AgentStatus::Blocked;
        pane.agent = Some("lcars-test".into());
        pane.terminal_title_stripped = Some("t".into());
        fleet.apply("h", SessionEvent::PaneRefreshed(Box::new(pane)), T0 + 4);
        assert_eq!(
            fleet.panes["h/w9:p2"].pending_prompt.as_deref(),
            Some("Which colour do you prefer?")
        );
        // Leaving blocked clears it.
        let working = status_events()
            .into_iter()
            .find(|e| e.agent_status == AgentStatus::Working)
            .unwrap();
        assert!(fleet.apply("h", SessionEvent::StatusChanged(working), T0 + 5));
        assert_eq!(fleet.panes["h/w9:p2"].pending_prompt, None);
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
