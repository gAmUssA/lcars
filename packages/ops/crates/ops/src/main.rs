//! `ops` binary: load config, run one Herdr client per session, merge, serve.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use ops::actions::ActionRelay;
use ops::banners::{Baseline, Notifier};
use ops::config::{self, Config};
use ops::herdr::client::SessionClient;
use ops::model::{Fleet, SessionEvent};
use ops::reveal::Revealer;
use ops::server::{self, FleetView, ServeOptions};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, mpsc, watch};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

const USAGE: &str = "\
ops — LCARS daemon: merges Herdr state from every session for the Stream Deck plugin

Usage:
  ops [--config PATH]        Run (default config: ~/.config/ops/ops.toml)
  ops --print-default-config Print the example ops.toml and exit
  ops --help

Logs go to stdout. Set RUST_LOG (e.g. RUST_LOG=ops=debug) to adjust.
The API token is written to <state dir>/token (OPS_STATE_DIR, default ~/.local/state/ops).";

#[derive(Debug)]
enum Cli {
    Run { config: PathBuf },
    PrintDefaultConfig,
    Help,
}

fn parse_args(args: &[String]) -> Result<Cli> {
    let mut config = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--help" | "-h" => return Ok(Cli::Help),
            "--print-default-config" => return Ok(Cli::PrintDefaultConfig),
            "--config" | "-c" => {
                let value = iter
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--config needs a path"))?;
                config = Some(PathBuf::from(value));
            }
            other => bail!("unknown argument `{other}`\n\n{USAGE}"),
        }
    }
    Ok(Cli::Run {
        config: config.unwrap_or_else(config::default_path),
    })
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args)? {
        Cli::Help => {
            println!("{USAGE}");
            Ok(())
        }
        Cli::PrintDefaultConfig => {
            print!("{}", config::DEFAULT_CONFIG);
            Ok(())
        }
        Cli::Run { config } => run(config).await,
    }
}

async fn run(config_path: PathBuf) -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let config: Config = config::load(&config_path)?;
    let state_dir = config::state_dir();
    let token = server::load_or_create_token(&state_dir)
        .with_context(|| format!("cannot create the API token under {}", state_dir.display()))?;
    info!(config = %config_path.display(), sessions = config.sessions.len(), token_file = %server::token_path(&state_dir).display(), "ops starting");

    let listener = TcpListener::bind(&config.listen).await.with_context(|| {
        format!(
            "cannot listen on {} — is another ops running? change `listen` in ops.toml",
            config.listen
        )
    })?;

    if let Ok(text) = std::fs::read_to_string(&config_path) {
        let legacy = config::legacy_keys(&text);
        if !legacy.is_empty() {
            warn!(config = %config_path.display(), keys = %legacy.join(", "), "ops.toml uses old key names; they still work, rename them when convenient");
        }
    }

    let mut fleet = Fleet::with_sessions(
        config
            .sessions
            .iter()
            .map(|s| (s.name.as_str(), s.machine.as_deref())),
        now_ms(),
    );
    let (view_tx, view_rx) = watch::channel(Arc::new(FleetView::from_fleet(&fleet)));
    let (updates, _) = broadcast::channel(server::UPDATE_BUFFER);
    let (event_tx, mut event_rx) = mpsc::channel::<(String, SessionEvent)>(256);

    let mut transports = std::collections::HashMap::new();
    let mut local_sockets = std::collections::HashMap::new();
    for session in &config.sessions {
        if let (true, Some(socket)) = (session.is_local(), &session.socket) {
            local_sockets.insert(session.name.clone(), config::expand_tilde(socket));
        }
        let transport = session
            .transport(&state_dir)
            .with_context(|| format!("session `{}`", session.name))?;
        info!(session = %session.name, via = %transport.describe(), "starting herdr client");
        transports.insert(session.name.clone(), transport.clone());
        tokio::spawn(SessionClient::new(session.name.clone(), transport).run(event_tx.clone()));
    }
    drop(event_tx);
    let relay = Arc::new(ActionRelay::new(transports));
    let revealer = Arc::new(Revealer::new(local_sockets));
    let notifier = Notifier::from_config(
        &config.banners,
        view_rx.clone(),
        relay.clone(),
        revealer.clone(),
    )?;
    let mut baseline = Baseline::default();
    if notifier.is_some() {
        info!("desktop banners on; herdr-notify will defer");
    }
    let options = ServeOptions {
        banners: notifier.is_some(),
        revealer,
    };

    let serve_updates = updates.clone();
    tokio::spawn(async move {
        if let Err(err) =
            server::serve(listener, view_rx, serve_updates, relay, &token, options).await
        {
            warn!(%err, "plugin server stopped");
        }
    });

    let shutdown = tokio::signal::ctrl_c();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            _ = &mut shutdown => {
                info!("shutting down");
                return Ok(());
            }
            next = event_rx.recv() => {
                let Some((session, event)) = next else { bail!("all session clients stopped") };
                log_event(&session, &event);
                if fleet.apply(&session, event, now_ms()) {
                    let view = Arc::new(FleetView::from_fleet(&fleet));
                    if let Some(notifier) = &notifier {
                        let flips = baseline.advance(&view);
                        notifier.show(flips, view.sessions.len() > 1);
                    }
                    server::publish(&view_tx, &updates, view);
                }
            }
        }
    }
}

fn log_event(session: &str, event: &SessionEvent) {
    match event {
        SessionEvent::Connected { version, protocol } => {
            info!(session, version, protocol, "session connected")
        }
        SessionEvent::Disconnected { error } => warn!(session, error, "session disconnected"),
        SessionEvent::Snapshot { panes, agents } => {
            info!(
                session,
                panes = panes.len(),
                agents = agents.len(),
                "snapshot"
            )
        }
        SessionEvent::StatusChanged(e) => info!(
            session,
            pane = %e.pane_id,
            agent = e.agent.as_deref().unwrap_or("-"),
            status = ?e.agent_status,
            title = e.title.as_deref().unwrap_or(""),
            "status"
        ),
        SessionEvent::PaneCreated(p) => info!(session, pane = %p.pane_id, "pane created"),
        SessionEvent::PaneClosed { pane_id } => info!(session, pane = %pane_id, "pane closed"),
        SessionEvent::AgentDetected {
            pane_id,
            agent,
            released,
        } => {
            info!(session, pane = %pane_id, agent = agent.as_deref().unwrap_or("-"), released, "agent detected")
        }
        SessionEvent::PaneRefreshed(p) => {
            info!(session, pane = %p.pane_id, title = p.terminal_title_stripped.as_deref().unwrap_or(""), "pane refreshed")
        }
        SessionEvent::AgentRefreshed(a) => {
            info!(session, pane = %a.pane_id, status = ?a.agent_status, done = a.completion_seq == Some(a.state_change_seq) && !a.focused, "agent refreshed")
        }
        SessionEvent::PaneFocused { pane_id } => info!(session, pane = %pane_id, "pane focused"),
        SessionEvent::Workspaces(list) => info!(session, workspaces = list.len(), "workspaces"),
        SessionEvent::PromptRead { pane_id, prompt } => {
            info!(session, pane = %pane_id, prompt = prompt.as_deref().unwrap_or("-"), "prompt read")
        }
        SessionEvent::WorkspacesChanged => info!(session, "workspaces changed"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn no_args_runs_with_default_config_path() {
        match parse_args(&[]).unwrap() {
            Cli::Run { config } => assert_eq!(config, config::default_path()),
            _ => panic!("expected run"),
        }
    }

    #[test]
    fn config_flag_sets_the_path() {
        match parse_args(&args(&["--config", "/x/ops.toml"])).unwrap() {
            Cli::Run { config } => assert_eq!(config, PathBuf::from("/x/ops.toml")),
            _ => panic!("expected run"),
        }
    }

    #[test]
    fn print_default_config_and_help_are_recognised() {
        assert!(matches!(
            parse_args(&args(&["--print-default-config"])).unwrap(),
            Cli::PrintDefaultConfig
        ));
        assert!(matches!(parse_args(&args(&["-h"])).unwrap(), Cli::Help));
    }

    #[test]
    fn unknown_argument_fails_with_usage() {
        let err = parse_args(&args(&["--poll"])).unwrap_err();
        assert!(err.to_string().contains("unknown argument `--poll`"));
        assert!(err.to_string().contains("Usage:"));
    }

    #[test]
    fn config_without_value_fails() {
        assert!(parse_args(&args(&["--config"])).is_err());
    }
}
