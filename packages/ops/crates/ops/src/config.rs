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
    #[serde(default)]
    pub hosts: Vec<HostConfig>,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HostConfig {
    pub name: String,
    /// Local Herdr socket path. Mutually exclusive with `ssh`.
    pub socket: Option<String>,
    /// SSH alias from `~/.ssh/config`. Mutually exclusive with `socket`.
    pub ssh: Option<String>,
    /// Absolute Herdr socket path on the remote host. Required with `ssh`.
    pub remote_socket: Option<String>,
    /// SSH ControlMaster socket. Defaults to `~/.ssh/cm-<ssh>`.
    pub control_path: Option<String>,
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
            "config file not found at {} — run `ops --print-default-config > {}` and edit the hosts",
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
        if self.hosts.is_empty() {
            bail!("no [[hosts]] configured — add at least one host with `socket` or `ssh`");
        }
        let mut seen = std::collections::BTreeSet::new();
        for host in &self.hosts {
            host.validate()?;
            if !seen.insert(host.name.as_str()) {
                bail!(
                    "duplicate host name `{}` — host names must be unique",
                    host.name
                );
            }
        }
        Ok(())
    }
}

impl HostConfig {
    pub fn validate(&self) -> Result<()> {
        if self.name.is_empty() {
            bail!("a [[hosts]] entry has an empty `name`");
        }
        match (&self.socket, &self.ssh) {
            (Some(_), Some(_)) => bail!(
                "host `{}` sets both `socket` and `ssh` — pick one",
                self.name
            ),
            (None, None) => bail!(
                "host `{}` needs either `socket` (local) or `ssh` (remote alias)",
                self.name
            ),
            (None, Some(_)) => match &self.remote_socket {
                Some(remote) if remote.starts_with('/') => Ok(()),
                Some(remote) => bail!(
                    "host `{}`: `remote_socket` must be absolute, got `{}` (the remote side cannot expand ~)",
                    self.name,
                    remote
                ),
                None => bail!(
                    "host `{}` uses `ssh` but has no `remote_socket` — set it to the absolute Herdr socket path on that machine",
                    self.name
                ),
            },
            (Some(_), None) => Ok(()),
        }
    }

    /// Build the transport for this host. `state_dir` holds forwarded sockets.
    pub fn transport(&self, state_dir: &Path) -> Result<Transport> {
        if let Some(socket) = &self.socket {
            return Ok(Transport::Local {
                socket: expand_tilde(socket),
            });
        }
        let alias = self.ssh.clone().expect("validated: ssh present");
        let remote_socket = self
            .remote_socket
            .clone()
            .expect("validated: remote_socket present");
        let control_path = match &self.control_path {
            Some(path) => expand_tilde(path),
            None => expand_tilde(&format!("~/.ssh/cm-{alias}")),
        };
        Transport::ssh(
            alias,
            remote_socket,
            control_path,
            state_dir.join(format!("{}.sock", self.name)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn example_config_parses_with_two_hosts() {
        let config = parse(DEFAULT_CONFIG).expect("example config must parse");
        assert_eq!(config.listen, "127.0.0.1:7677");
        assert_eq!(config.hosts.len(), 2);
        assert_eq!(config.hosts[0].name, "studio");
        assert_eq!(config.hosts[1].ssh.as_deref(), Some("mini"));
    }

    #[test]
    fn listen_defaults_when_omitted() {
        let config = parse("[[hosts]]\nname = \"a\"\nsocket = \"/tmp/h.sock\"\n").unwrap();
        assert_eq!(config.listen, DEFAULT_LISTEN);
    }

    #[test]
    fn rejects_non_loopback_listen_unless_opted_in() {
        let err = parse("listen = \"0.0.0.0:7677\"\n[[hosts]]\nname = \"a\"\nsocket = \"/a\"\n")
            .unwrap_err();
        assert!(err.to_string().contains("loopback"), "{err}");
        let ok = parse("listen = \"0.0.0.0:7677\"\nallow_remote_listen = true\n[[hosts]]\nname = \"a\"\nsocket = \"/a\"\n");
        assert!(ok.is_ok());
        let err =
            parse("listen = \"nonsense\"\n[[hosts]]\nname = \"a\"\nsocket = \"/a\"\n").unwrap_err();
        assert!(err.to_string().contains("host:port"), "{err}");
    }

    #[test]
    fn rejects_empty_host_list() {
        let err = parse("listen = \"127.0.0.1:1\"\n").unwrap_err();
        assert!(err.to_string().contains("no [[hosts]]"), "{err}");
    }

    #[test]
    fn rejects_host_with_both_socket_and_ssh() {
        let err = parse(
            "[[hosts]]\nname = \"x\"\nsocket = \"/a\"\nssh = \"x\"\nremote_socket = \"/b\"\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("both `socket` and `ssh`"), "{err}");
    }

    #[test]
    fn rejects_ssh_host_without_remote_socket() {
        let err = parse("[[hosts]]\nname = \"x\"\nssh = \"x\"\n").unwrap_err();
        assert!(err.to_string().contains("no `remote_socket`"), "{err}");
    }

    #[test]
    fn rejects_relative_remote_socket() {
        let err = parse("[[hosts]]\nname = \"x\"\nssh = \"x\"\nremote_socket = \"~/h.sock\"\n")
            .unwrap_err();
        assert!(err.to_string().contains("must be absolute"), "{err}");
    }

    #[test]
    fn rejects_duplicate_host_names() {
        let err = parse(
            "[[hosts]]\nname = \"a\"\nsocket = \"/a\"\n[[hosts]]\nname = \"a\"\nsocket = \"/b\"\n",
        )
        .unwrap_err();
        assert!(err.to_string().contains("duplicate host name"), "{err}");
    }

    #[test]
    fn rejects_unknown_keys() {
        let err = parse("[[hosts]]\nname = \"a\"\nsocket = \"/a\"\npoll_ms = 3000\n").unwrap_err();
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
    fn ssh_host_transport_defaults_control_path_from_alias() {
        let host = HostConfig {
            name: "mini".into(),
            socket: None,
            ssh: Some("mini".into()),
            remote_socket: Some("/Users/v/.config/herdr/herdr.sock".into()),
            control_path: None,
        };
        let transport = host.transport(Path::new("/tmp/ops-state")).unwrap();
        match transport {
            Transport::Ssh {
                alias,
                control_path,
                local_socket,
                ..
            } => {
                assert_eq!(alias, "mini");
                assert_eq!(control_path, expand_tilde("~/.ssh/cm-mini"));
                assert_eq!(local_socket, PathBuf::from("/tmp/ops-state/mini.sock"));
            }
            other => panic!("expected ssh transport, got {other:?}"),
        }
    }
}
