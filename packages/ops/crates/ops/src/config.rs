//! `ops.toml` loading and validation.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::herdr::transport::Transport;

/// The example shipped beside the crate; printed by `ops --print-default-config`.
pub const DEFAULT_CONFIG: &str = include_str!("../ops.toml.example");

/// Default loopback address for the plugin-facing server.
pub const DEFAULT_LISTEN: &str = "127.0.0.1:7677";

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default = "default_listen")]
    pub listen: String,
    /// The API carries key presses into agents; it stays on loopback unless
    /// the operator opts in explicitly.
    #[serde(default)]
    pub allow_remote_listen: bool,
    #[serde(default, alias = "hosts")]
    pub sessions: Vec<SessionConfig>,
    #[serde(default)]
    pub banners: BannersConfig,
}

/// Desktop banners for blocked and finished agents (`[banners]`). Off by
/// default: with it on, Ops tells herdr-notify to stay silent (see
/// `docs/adr/0002-desktop-banners.md`).
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BannersConfig {
    #[serde(default)]
    pub enabled: bool,
    /// Path to `alerter`. Default: PATH, then the Homebrew prefixes, because
    /// launchd starts Ops with a minimal PATH.
    pub alerter: Option<String>,
    /// Image shown on the banner.
    pub icon: Option<String>,
    /// Play Funk (blocked) or Glass (done) with the banner.
    #[serde(default = "default_true")]
    pub sound: bool,
}

impl Default for BannersConfig {
    fn default() -> Self {
        BannersConfig {
            enabled: false,
            alerter: None,
            icon: None,
            sound: true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// One Herdr session: one Herdr server and its socket, on this machine or
/// on a `machine` reached over SSH. Named like Herdr: `herdr session list`
/// shows the sockets, `herdr --remote <machine>` reaches the same server.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SessionConfig {
    /// Label on the deck and in the API. Unique across the config.
    pub name: String,
    /// SSH alias from `~/.ssh/config` for a session on another machine.
    /// Absent: the session runs on this machine.
    #[serde(alias = "ssh")]
    pub machine: Option<String>,
    /// Herdr socket path on the session's machine. Absolute when `machine`
    /// is set (the remote side cannot expand `~`).
    #[serde(alias = "remote_socket")]
    pub socket: Option<String>,
    /// SSH ControlMaster socket. Defaults to `~/.ssh/cm-<machine>`.
    pub control_path: Option<String>,
}

/// Config keys from before sessions were named like Herdr's; they still
/// parse. Returns the ones `text` uses, for a startup warning.
pub fn legacy_keys(text: &str) -> Vec<&'static str> {
    let Ok(value) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let mut found = Vec::new();
    if value.contains_key("hosts") {
        found.push("[[hosts]] (now [[sessions]])");
    }
    let entries = value
        .get("hosts")
        .or_else(|| value.get("sessions"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    let uses = |key: &str| entries.iter().any(|e| e.get(key).is_some());
    if uses("ssh") {
        found.push("ssh (now machine)");
    }
    if uses("remote_socket") {
        found.push("remote_socket (now socket)");
    }
    found
}

fn default_listen() -> String {
    DEFAULT_LISTEN.to_string()
}

/// Default config location: `~/.config/ops/ops.toml`.
pub fn default_path() -> PathBuf {
    expand_tilde("~/.config/ops/ops.toml")
}

/// Directory for Ops-owned runtime files such as forwarded sockets.
/// `OPS_STATE_DIR` overrides; default `~/.local/state/ops`.
pub fn state_dir() -> PathBuf {
    match std::env::var("OPS_STATE_DIR") {
        Ok(dir) if !dir.is_empty() => expand_tilde(&dir),
        _ => expand_tilde("~/.local/state/ops"),
    }
}

/// Expand a leading `~` or `~/` using `HOME`. Anything else is returned as-is.
pub fn expand_tilde(value: &str) -> PathBuf {
    if value == "~" || value.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            let rest = value.trim_start_matches('~').trim_start_matches('/');
            return if rest.is_empty() {
                PathBuf::from(home)
            } else {
                Path::new(&home).join(rest)
            };
        }
    }
    PathBuf::from(value)
}

pub fn load(path: &Path) -> Result<Config> {
    let text = std::fs::read_to_string(path).with_context(|| {
        format!(
            "config file not found at {} — run `ops --print-default-config > {}` and edit the sessions",
            path.display(),
            path.display()
        )
    })?;
    parse(&text).with_context(|| format!("invalid config at {}", path.display()))
}

pub fn parse(text: &str) -> Result<Config> {
    let config: Config = toml::from_str(text).context("TOML parse error")?;
    config.validate()?;
    Ok(config)
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        let addr: std::net::SocketAddr = self.listen.parse().map_err(|e| {
            anyhow::anyhow!("`listen` must be host:port (got `{}`): {e}", self.listen)
        })?;
        if !addr.ip().is_loopback() && !self.allow_remote_listen {
            bail!(
                "`listen = \"{}\"` is not a loopback address; the API relays key presses into agents. Use 127.0.0.1 or set `allow_remote_listen = true` knowingly",
                self.listen
            );
        }
        if self.sessions.is_empty() {
            bail!("no [[sessions]] configured — add one with `socket` (this machine) or `machine` plus `socket` (over SSH)");
        }
        let mut seen = std::collections::BTreeSet::new();
        for session in &self.sessions {
            session.validate()?;
            if !seen.insert(session.name.as_str()) {
                bail!(
                    "duplicate session name `{}` — session names must be unique",
                    session.name
                );
            }
        }
        Ok(())
    }
}

impl SessionConfig {
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            bail!("a [[sessions]] entry has an empty `name`");
        }
        let Some(socket) = &self.socket else {
            bail!(
                "session `{}` has no `socket` — set it to the Herdr socket path (`herdr session list` prints it)",
                self.name
            );
        };
        if let Some(machine) = &self.machine {
            if !socket.starts_with('/') {
                bail!(
                    "session `{}`: `socket` must be absolute on machine `{machine}`, got `{socket}` (the remote side cannot expand ~)",
                    self.name
                );
            }
        }
        Ok(())
    }

    /// True when the session's Herdr runs on this machine.
    pub fn is_local(&self) -> bool {
        self.machine.is_none()
    }

    /// Build the transport for this session. `state_dir` holds forwarded sockets.
    pub fn transport(&self, state_dir: &Path) -> Result<Transport> {
        let socket = self.socket.clone().expect("validated: socket present");
        let Some(machine) = self.machine.clone() else {
            return Ok(Transport::Local {
                socket: expand_tilde(&socket),
            });
        };
        let control_path = match &self.control_path {
            Some(path) => expand_tilde(path),
            None => expand_tilde(&format!("~/.ssh/cm-{machine}")),
        };
        Transport::ssh(
            machine,
            socket,
            control_path,
            state_dir.join(format!("{}.sock", self.name)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOCAL: &str = "[[sessions]]\nname = \"a\"\nsocket = \"/a\"\n";

    #[test]
    fn example_config_parses_with_a_local_and_a_remote_session() {
        let config = parse(DEFAULT_CONFIG).expect("example config must parse");
        assert_eq!(config.listen, "127.0.0.1:7677");
        assert_eq!(config.sessions.len(), 3);
        assert_eq!(config.sessions[0].name, "studio");
        assert!(config.sessions[0].is_local());
        assert_eq!(config.sessions[1].machine.as_deref(), Some("mini"));
        assert!(legacy_keys(DEFAULT_CONFIG).is_empty());
    }

    #[test]
    fn listen_defaults_when_omitted() {
        let config = parse("[[sessions]]\nname = \"a\"\nsocket = \"/tmp/h.sock\"\n").unwrap();
        assert_eq!(config.listen, DEFAULT_LISTEN);
    }

    #[test]
    fn rejects_non_loopback_listen_unless_opted_in() {
        let err = parse(&format!("listen = \"0.0.0.0:7677\"\n{LOCAL}")).unwrap_err();
        assert!(err.to_string().contains("loopback"), "{err}");
        let ok = parse(&format!(
            "listen = \"0.0.0.0:7677\"\nallow_remote_listen = true\n{LOCAL}"
        ));
        assert!(ok.is_ok());
        let err = parse(&format!("listen = \"nonsense\"\n{LOCAL}")).unwrap_err();
        assert!(err.to_string().contains("host:port"), "{err}");
    }

    #[test]
    fn rejects_empty_session_list() {
        let err = parse("listen = \"127.0.0.1:1\"\n").unwrap_err();
        assert!(err.to_string().contains("no [[sessions]]"), "{err}");
    }

    #[test]
    fn rejects_session_without_socket() {
        let err = parse("[[sessions]]\nname = \"x\"\nmachine = \"x\"\n").unwrap_err();
        assert!(err.to_string().contains("no `socket`"), "{err}");
        assert!(err.to_string().contains("herdr session list"), "{err}");
    }

    #[test]
    fn remote_socket_must_be_absolute_local_may_use_tilde() {
        let err = parse("[[sessions]]\nname = \"x\"\nmachine = \"x\"\nsocket = \"~/h.sock\"\n")
            .unwrap_err();
        assert!(err.to_string().contains("must be absolute"), "{err}");
        let ok = parse("[[sessions]]\nname = \"x\"\nsocket = \"~/h.sock\"\n").unwrap();
        assert!(ok.sessions[0].is_local());
    }

    #[test]
    fn legacy_host_config_still_parses_and_is_reported() {
        let old = "[[hosts]]\nname = \"studio\"\nsocket = \"~/s.sock\"\n[[hosts]]\nname = \"mini\"\nssh = \"ds\"\nremote_socket = \"/r.sock\"\ncontrol_path = \"~/.ssh/cm-ds\"\n";
        let config = parse(old).unwrap();
        assert_eq!(config.sessions.len(), 2);
        assert_eq!(config.sessions[1].machine.as_deref(), Some("ds"));
        assert_eq!(config.sessions[1].socket.as_deref(), Some("/r.sock"));
        assert_eq!(
            legacy_keys(old),
            vec![
                "[[hosts]] (now [[sessions]])",
                "ssh (now machine)",
                "remote_socket (now socket)"
            ]
        );
    }

    #[test]
    fn old_and_new_key_for_one_field_is_an_error() {
        let err =
            parse("[[sessions]]\nname = \"x\"\nmachine = \"a\"\nssh = \"b\"\nsocket = \"/s\"\n")
                .unwrap_err();
        assert!(format!("{err:#}").contains("duplicate"), "{err:#}");
    }

    #[test]
    fn rejects_duplicate_session_names() {
        let err = parse(&format!("{LOCAL}{LOCAL}")).unwrap_err();
        assert!(err.to_string().contains("duplicate session name"), "{err}");
    }

    #[test]
    fn banners_default_off_with_sound() {
        let config = parse(LOCAL).unwrap();
        assert_eq!(config.banners, BannersConfig::default());
        assert!(!config.banners.enabled);
        assert!(config.banners.sound);
    }

    #[test]
    fn banners_section_parses_and_rejects_typos() {
        let config = parse(&format!(
            "[banners]\nenabled = true\nsound = false\nalerter = \"/x/alerter\"\n{LOCAL}"
        ))
        .unwrap();
        assert!(config.banners.enabled);
        assert!(!config.banners.sound);
        assert_eq!(config.banners.alerter.as_deref(), Some("/x/alerter"));
        assert!(parse(&format!("[banners]\nenable = true\n{LOCAL}")).is_err());
    }

    #[test]
    fn rejects_unknown_keys() {
        let err = parse(&format!("{LOCAL}poll_ms = 3000\n")).unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("poll_ms"), "{text}");
    }

    #[test]
    fn missing_file_error_names_the_fix() {
        let err = load(Path::new("/nonexistent/ops.toml")).unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("--print-default-config"), "{text}");
    }

    #[test]
    fn tilde_expands_against_home_only_at_start() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_tilde("~/x/y"), Path::new(&home).join("x/y"));
        assert_eq!(expand_tilde("~"), PathBuf::from(&home));
        assert_eq!(expand_tilde("/abs/~/x"), PathBuf::from("/abs/~/x"));
        assert_eq!(expand_tilde("~user/x"), PathBuf::from("~user/x"));
    }

    #[test]
    fn remote_session_transport_defaults_control_path_from_machine() {
        let session = SessionConfig {
            name: "talk".into(),
            machine: Some("mini".into()),
            socket: Some("/Users/v/.config/herdr/sessions/talk/herdr.sock".into()),
            control_path: None,
        };
        let transport = session.transport(Path::new("/tmp/ops-state")).unwrap();
        match transport {
            Transport::Ssh {
                alias,
                control_path,
                local_socket,
                ..
            } => {
                assert_eq!(alias, "mini");
                assert_eq!(control_path, expand_tilde("~/.ssh/cm-mini"));
                assert_eq!(local_socket, PathBuf::from("/tmp/ops-state/talk.sock"));
            }
            other => panic!("expected ssh transport, got {other:?}"),
        }
    }
}
