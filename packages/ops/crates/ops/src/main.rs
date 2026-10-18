//! `ops` binary: load config, run one Herdr client per host, merge, serve.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use ops::actions::ActionRelay;
use ops::config::{self, Config};
use ops::herdr::client::HostClient;
use ops::model::{Fleet, HostEvent};
use ops::server::{self, FleetView};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

const USAGE: &str = "\
ops — LCARS daemon: merges Herdr state from every host for the Stream Deck plugin

Usage:
  ops [--config PATH]        Run (default config: ~/.config/ops/ops.toml)
  ops --print-default-config Print the example ops.toml and exit
  ops --help

Logs go to stdout. Set RUST_LOG (e.g. RUST_LOG=ops=debug) to adjust.";

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
    info!(config = %config_path.display(), hosts = config.hosts.len(), "ops starting");

    let listener = TcpListener::bind(&config.listen).await.with_context(|| {
        format!(
            "cannot listen on {} — is another ops running? change `listen` in ops.toml",
            config.listen
        )
    })?;

    let mut fleet = Fleet::with_hosts(config.hosts.iter().map(|h| h.name.as_str()), now_ms());
    let (view_tx, view_rx) = watch::channel(Arc::new(FleetView::from_fleet(&fleet)));
    let (event_tx, mut event_rx) = mpsc::channel::<(String, HostEvent)>(256);

    let mut transports = std::collections::HashMap::new();
    for host in &config.hosts {
        let transport = host
            .transport(&state_dir)
            .with_context(|| format!("host `{}`", host.name))?;
        info!(host = %host.name, via = %transport.describe(), "starting herdr client");
        transports.insert(host.name.clone(), transport.clone());
        tokio::spawn(HostClient::new(host.name.clone(), transport).run(event_tx.clone()));
    }
    drop(event_tx);
    let relay = Arc::new(ActionRelay::new(transports));

    tokio::spawn(async move {
        if let Err(err) = server::serve(listener, view_rx, relay).await {
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
                let Some((host, event)) = next else { bail!("all host clients stopped") };
                log_event(&host, &event);
                if fleet.apply(&host, event, now_ms()) {
                    let _ = view_tx.send(Arc::new(FleetView::from_fleet(&fleet)));
                }
            }
        }
    }
}

fn log_event(host: &str, event: &HostEvent) {
    match event {
        HostEvent::Connected { version, protocol } => {
            info!(host, version, protocol, "host connected")
        }
        HostEvent::Disconnected { error } => warn!(host, error, "host disconnected"),
        HostEvent::Snapshot { panes, agents } => {
            info!(host, panes = panes.len(), agents = agents.len(), "snapshot")
        }
        HostEvent::StatusChanged(e) => info!(
            host,
            pane = %e.pane_id,
            agent = e.agent.as_deref().unwrap_or("-"),
            status = ?e.agent_status,
            title = e.title.as_deref().unwrap_or(""),
            "status"
        ),
        HostEvent::PaneCreated(p) => info!(host, pane = %p.pane_id, "pane created"),
        HostEvent::PaneClosed { pane_id } => info!(host, pane = %pane_id, "pane closed"),
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
