//! Desktop banners when an agent blocks or finishes a turn, on any session.
//!
//! Close to the plugin's done ping (`core/transitions.ts`,
//! `core/ping-baseline.ts`): entering `blocked`, or `working` → `done`. Unlike
//! the ping, `working` → `idle` does not ring: Herdr reports `idle` for a turn
//! that finished in the pane you are looking at, and a banner for that is noise.
//! A session's first frame with panes after it (re)connects is a backlog and
//! only primes the baseline. Banners go through `alerter`; a click focuses the
//! pane through the action relay and reveals its iTerm session, so a banner
//! for a remote agent works from the deck session. See ADR 0002.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use tracing::{info, warn};

use crate::actions::ActionRelay;
use crate::config::{expand_tilde, BannersConfig};
use crate::model::{AgentState, Pane};
use crate::reveal::Revealer;
use crate::server::{FleetRx, FleetView};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlipKind {
    Blocked,
    Done,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Flip {
    pub pane: Pane,
    pub kind: FlipKind,
}

/// Panes that entered `blocked` or finished a turn between two lists. A
/// pane not seen before counts only if it is blocked.
pub fn detect_flips(prev: &[Pane], next: &[Pane]) -> Vec<Flip> {
    let before: HashMap<&str, AgentState> = prev.iter().map(|s| (s.id.as_str(), s.state)).collect();
    next.iter()
        .filter_map(|s| {
            let was = before.get(s.id.as_str()).copied();
            let kind = match s.state {
                AgentState::Blocked if was != Some(AgentState::Blocked) => FlipKind::Blocked,
                AgentState::Done if was == Some(AgentState::Working) => FlipKind::Done,
                _ => return None,
            };
            Some(Flip {
                pane: s.clone(),
                kind,
            })
        })
        .collect()
}

/// Consecutive views, folded so a reconnect backlog never rings.
#[derive(Debug, Default)]
pub struct Baseline {
    previous: Option<Vec<Pane>>,
    seeded: HashSet<String>,
}

impl Baseline {
    pub fn advance(&mut self, view: &FleetView) -> Vec<Flip> {
        // A session that dropped will deliver a backlog again when it returns.
        for session in view.sessions.iter().filter(|h| !h.connected) {
            self.seeded.remove(&session.name);
        }
        let has_panes = |session: &str| view.panes.iter().any(|s| s.session == session);
        let Some(mut previous) = self.previous.take() else {
            for session in view.sessions.iter().filter(|h| h.connected) {
                if has_panes(&session.name) {
                    self.seeded.insert(session.name.clone());
                }
            }
            self.previous = Some(view.panes.clone());
            return Vec::new();
        };
        for session in view.sessions.iter() {
            // Ops keeps a dropped session's last panes; not a backlog until it is back.
            if !session.connected
                || self.seeded.contains(&session.name)
                || !has_panes(&session.name)
            {
                continue;
            }
            previous.retain(|s| s.session != session.name);
            previous.extend(
                view.panes
                    .iter()
                    .filter(|s| s.session == session.name)
                    .cloned(),
            );
            self.seeded.insert(session.name.clone());
        }
        let flips = detect_flips(&previous, &view.panes);
        self.previous = Some(view.panes.clone());
        flips
    }
}

/// What one banner says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Banner {
    pub title: String,
    pub subtitle: String,
    pub message: String,
    pub sound: &'static str,
    /// alerter replaces an earlier banner with the same group.
    pub group: String,
}

pub fn banner_for(flip: &Flip, multi_host: bool) -> Banner {
    let s = &flip.pane;
    let label = s
        .name
        .clone()
        .or_else(|| s.agent.clone())
        .unwrap_or_else(|| "agent".to_string());
    let project = s
        .cwd
        .as_deref()
        .and_then(|cwd| Path::new(cwd).file_name())
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or(&s.workspace_id)
        .to_string();
    let subtitle = if multi_host {
        format!("{project} · {}", s.session)
    } else {
        project
    };
    let (message, sound) = match flip.kind {
        FlipKind::Blocked => (
            s.pending_prompt
                .clone()
                .unwrap_or_else(|| "needs your input".to_string()),
            "Funk",
        ),
        FlipKind::Done => ("turn complete".to_string(), "Glass"),
    };
    Banner {
        title: format!("herdr · {label}"),
        subtitle,
        message,
        sound,
        group: format!("lcars:{}", s.id),
    }
}

/// alerter's arguments. Values that start with `-` get a leading space so a
/// blocked question can never read as a flag.
pub fn alerter_args(banner: &Banner, icon: Option<&Path>, sound: bool) -> Vec<String> {
    let value = |v: &str| {
        if v.starts_with('-') {
            format!(" {v}")
        } else {
            v.to_string()
        }
    };
    let mut args = vec![
        "--title".into(),
        value(&banner.title),
        "--subtitle".into(),
        value(&banner.subtitle),
        "--message".into(),
        value(&banner.message),
        "--group".into(),
        banner.group.clone(),
        "--ignore-dnd".into(),
        "--timeout".into(),
        "30".into(),
    ];
    if sound {
        args.extend(["--sound".into(), banner.sound.to_string()]);
    }
    if let Some(icon) = icon {
        args.extend(["--app-icon".into(), icon.display().to_string()]);
    }
    args
}

/// Configured path, else PATH, else the Homebrew prefixes.
pub fn find_alerter(configured: Option<&str>) -> Option<PathBuf> {
    if let Some(path) = configured {
        return Some(expand_tilde(path));
    }
    let on_path = std::env::var_os("PATH")
        .map(|p| {
            std::env::split_paths(&p)
                .map(|d| d.join("alerter"))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    on_path
        .into_iter()
        .chain([
            PathBuf::from("/opt/homebrew/bin/alerter"),
            PathBuf::from("/usr/local/bin/alerter"),
        ])
        .find(|p| p.is_file())
}

pub struct Notifier {
    alerter: PathBuf,
    icon: Option<PathBuf>,
    sound: bool,
    fleet: FleetRx,
    relay: Arc<ActionRelay>,
    revealer: Arc<Revealer>,
}

impl Notifier {
    /// `None` when banners are off; an error when they are on but alerter is missing.
    pub fn from_config(
        config: &BannersConfig,
        fleet: FleetRx,
        relay: Arc<ActionRelay>,
        revealer: Arc<Revealer>,
    ) -> anyhow::Result<Option<Notifier>> {
        if !config.enabled {
            return Ok(None);
        }
        let alerter = find_alerter(config.alerter.as_deref()).ok_or_else(|| {
            anyhow::anyhow!(
                "[banners] is enabled but alerter was not found — brew install vjeantet/tap/alerter, or set `alerter` in ops.toml"
            )
        })?;
        Ok(Some(Notifier {
            alerter,
            icon: config.icon.as_deref().map(expand_tilde),
            sound: config.sound,
            fleet,
            relay,
            revealer,
        }))
    }

    /// Raise one banner per flip; each waits for its own click in the background.
    pub fn show(&self, flips: Vec<Flip>, multi_host: bool) {
        for flip in flips {
            let banner = banner_for(&flip, multi_host);
            let args = alerter_args(&banner, self.icon.as_deref(), self.sound);
            let alerter = self.alerter.clone();
            let fleet = self.fleet.clone();
            let relay = self.relay.clone();
            let revealer = self.revealer.clone();
            let target_id = flip.pane.id.clone();
            info!(pane = %target_id, kind = ?flip.kind, "banner");
            tokio::spawn(async move {
                let output = match tokio::process::Command::new(&alerter)
                    .args(&args)
                    .output()
                    .await
                {
                    Ok(output) => output,
                    Err(err) => {
                        warn!(alerter = %alerter.display(), %err, "cannot run alerter");
                        return;
                    }
                };
                if String::from_utf8_lossy(&output.stdout).trim() != "@CONTENTCLICKED" {
                    return;
                }
                if let Err(err) = open(&fleet, &relay, &revealer, &target_id).await {
                    warn!(pane = %target_id, error = %format!("{err:#}"), "banner click failed");
                }
            });
        }
    }
}

/// A banner click: focus the pane on its session, then put it on screen.
async fn open(
    fleet: &FleetRx,
    relay: &ActionRelay,
    revealer: &Revealer,
    target_id: &str,
) -> anyhow::Result<()> {
    let pane = {
        let view = fleet.borrow();
        let pane = view.panes.iter().find(|s| s.id == target_id).cloned();
        if let Some(s) = &pane {
            if !view
                .sessions
                .iter()
                .any(|h| h.name == s.session && h.connected)
            {
                anyhow::bail!("session `{}` is disconnected", s.session);
            }
        }
        pane
    };
    let Some(pane) = pane else {
        anyhow::bail!("{target_id} is gone");
    };
    relay.focus(&pane).await?;
    if !revealer.reveal(&pane.session).await? {
        // Remote session or no attached iTerm client: raise the app. `open -a`
        // needs no Automation consent.
        tokio::process::Command::new("/usr/bin/open")
            .args(["-a", "iTerm"])
            .status()
            .await?;
    }
    info!(pane = %target_id, "banner click focused");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::SessionStatus;

    fn pane(session: &str, pane: &str, state: AgentState) -> Pane {
        Pane {
            id: Pane::id_for(session, pane),
            session: session.into(),
            workspace_id: "w1".into(),
            tab_id: "w1:t1".into(),
            pane_id: pane.into(),
            agent: Some("claude".into()),
            name: None,
            state,
            title: None,
            cwd: Some("/Users/me/projects/lcars".into()),
            model: None,
            summary: None,
            focused: false,
            seq: 1,
            updated_at_ms: 0,
            pending_prompt: None,
        }
    }

    fn session(name: &str, connected: bool) -> SessionStatus {
        SessionStatus {
            name: name.into(),
            machine: None,
            local: true,
            connected,
            version: None,
            protocol: None,
            error: None,
            updated_at_ms: 0,
        }
    }

    fn view(sessions: Vec<SessionStatus>, panes: Vec<Pane>) -> FleetView {
        FleetView {
            api: crate::server::API_VERSION,
            sessions,
            panes,
            workspaces: vec![],
        }
    }

    #[test]
    fn flips_on_entering_blocked_and_finishing_a_turn() {
        let prev = vec![
            pane("a", "p1", AgentState::Working),
            pane("a", "p2", AgentState::Working),
            pane("a", "p3", AgentState::Blocked),
            pane("a", "p4", AgentState::Idle),
            pane("a", "p7", AgentState::Working),
        ];
        let next = vec![
            pane("a", "p1", AgentState::Blocked),
            pane("a", "p2", AgentState::Done),
            pane("a", "p3", AgentState::Blocked),
            pane("a", "p4", AgentState::Idle),
            pane("a", "p5", AgentState::Idle),
            pane("a", "p6", AgentState::Blocked),
            // Finished in the pane you are watching: Herdr says idle, not done.
            pane("a", "p7", AgentState::Idle),
        ];
        let flips: Vec<_> = detect_flips(&prev, &next)
            .into_iter()
            .map(|f| (f.pane.pane_id, f.kind))
            .collect();
        assert_eq!(
            flips,
            vec![
                ("p1".to_string(), FlipKind::Blocked),
                ("p2".to_string(), FlipKind::Done),
                ("p6".to_string(), FlipKind::Blocked),
            ]
        );
    }

    #[test]
    fn first_view_and_reconnect_backlog_only_prime() {
        let mut baseline = Baseline::default();
        let blocked = pane("a", "p1", AgentState::Blocked);
        assert!(baseline
            .advance(&view(vec![session("a", true)], vec![blocked.clone()]))
            .is_empty());
        // Session drops and comes back with a different backlog: still silent.
        baseline.advance(&view(vec![session("a", false)], vec![blocked.clone()]));
        let backlog = pane("a", "p2", AgentState::Blocked);
        assert!(baseline
            .advance(&view(
                vec![session("a", true)],
                vec![blocked.clone(), backlog.clone()]
            ))
            .is_empty());
        // After that, a real flip rings.
        let working = pane("a", "p3", AgentState::Working);
        baseline.advance(&view(
            vec![session("a", true)],
            vec![blocked.clone(), working],
        ));
        let flips = baseline.advance(&view(
            vec![session("a", true)],
            vec![blocked, pane("a", "p3", AgentState::Done)],
        ));
        assert_eq!(flips.len(), 1);
        assert_eq!(flips[0].kind, FlipKind::Done);
    }

    #[test]
    fn host_connecting_late_is_a_backlog_not_news() {
        let mut baseline = Baseline::default();
        baseline.advance(&view(vec![session("a", true), session("b", true)], vec![]));
        let flips = baseline.advance(&view(
            vec![session("a", true), session("b", true)],
            vec![pane("b", "p1", AgentState::Blocked)],
        ));
        assert!(flips.is_empty());
    }

    #[test]
    fn banner_names_agent_project_host_and_question() {
        let mut s = pane("mini", "p1", AgentState::Blocked);
        s.name = Some("reviewer".into());
        s.pending_prompt = Some("Allow edit to main.rs?".into());
        let banner = banner_for(
            &Flip {
                pane: s.clone(),
                kind: FlipKind::Blocked,
            },
            true,
        );
        assert_eq!(banner.title, "herdr · reviewer");
        assert_eq!(banner.subtitle, "lcars · mini");
        assert_eq!(banner.message, "Allow edit to main.rs?");
        assert_eq!(banner.sound, "Funk");
        assert_eq!(banner.group, "lcars:mini/p1");

        s.cwd = None;
        let done = banner_for(
            &Flip {
                pane: s,
                kind: FlipKind::Done,
            },
            false,
        );
        assert_eq!(done.subtitle, "w1");
        assert_eq!(done.message, "turn complete");
        assert_eq!(done.sound, "Glass");
    }

    #[test]
    fn alerter_args_guard_dashes_and_honour_sound_and_icon() {
        let banner = Banner {
            title: "herdr · claude".into(),
            subtitle: "lcars".into(),
            message: "--rm -rf?".into(),
            sound: "Funk",
            group: "lcars:a/p1".into(),
        };
        let args = alerter_args(&banner, Some(Path::new("/i.png")), false);
        let at = args.iter().position(|a| a == "--message").unwrap();
        assert_eq!(args[at + 1], " --rm -rf?");
        assert!(!args.contains(&"--sound".to_string()));
        assert!(args.ends_with(&["--app-icon".to_string(), "/i.png".to_string()]));
        assert!(alerter_args(&banner, None, true)
            .ends_with(&["--sound".to_string(), "Funk".to_string()]));
    }

    #[test]
    fn configured_alerter_path_wins() {
        assert_eq!(
            find_alerter(Some("/x/alerter")),
            Some(PathBuf::from("/x/alerter"))
        );
    }
}
