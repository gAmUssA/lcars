//! How Ops reaches a host's Herdr socket: directly, or through a Unix
//! socket forwarded over an existing SSH ControlMaster.

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use tokio::net::UnixStream;
use tokio::process::Command;

/// macOS `sun_path` is 104 bytes; leave room for the NUL.
pub const MAX_SOCKET_PATH_BYTES: usize = 103;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Transport {
    Local {
        socket: PathBuf,
    },
    Ssh {
        alias: String,
        remote_socket: String,
        control_path: PathBuf,
        local_socket: PathBuf,
    },
}

impl Transport {
    pub fn ssh(
        alias: String,
        remote_socket: String,
        control_path: PathBuf,
        local_socket: PathBuf,
    ) -> Result<Self> {
        check_socket_path(&local_socket)?;
        Ok(Transport::Ssh {
            alias,
            remote_socket,
            control_path,
            local_socket,
        })
    }

    /// Human label for logs.
    pub fn describe(&self) -> String {
        match self {
            Transport::Local { socket } => socket.display().to_string(),
            Transport::Ssh {
                alias,
                remote_socket,
                ..
            } => format!("ssh {alias}:{remote_socket}"),
        }
    }

    /// Open one fresh connection to the host's Herdr socket. For SSH
    /// transports this establishes the forward on first use and after the
    /// forwarded socket goes stale.
    pub async fn connect(&self) -> Result<UnixStream> {
        match self {
            Transport::Local { socket } => UnixStream::connect(socket).await.with_context(|| {
                format!(
                    "cannot connect to Herdr socket {} — is Herdr running on this machine? (`herdr status`)",
                    socket.display()
                )
            }),
            Transport::Ssh {
                alias,
                remote_socket,
                control_path,
                local_socket,
            } => {
                if let Ok(stream) = UnixStream::connect(local_socket).await {
                    return Ok(stream);
                }
                ensure_master(alias, control_path).await?;
                if let Some(dir) = local_socket.parent() {
                    tokio::fs::create_dir_all(dir)
                        .await
                        .with_context(|| format!("cannot create {}", dir.display()))?;
                }
                if tokio::fs::try_exists(local_socket).await.unwrap_or(false) {
                    // Stale forward from an earlier run. Cancel it on the master
                    // (ignore failure: it may already be gone) and unlink.
                    let _ = ssh_control(
                        control_path,
                        &["-O", "cancel", "-L", &forward_spec(local_socket, remote_socket), alias],
                    )
                    .await;
                    tokio::fs::remove_file(local_socket).await.with_context(|| {
                        format!("cannot remove stale socket {}", local_socket.display())
                    })?;
                }
                ssh_control(
                    control_path,
                    &["-O", "forward", "-L", &forward_spec(local_socket, remote_socket), alias],
                )
                .await
                .with_context(|| format!("ssh forward of {remote_socket} on {alias} failed"))?;
                UnixStream::connect(local_socket).await.with_context(|| {
                    format!(
                        "forward is up but {} does not answer — is Herdr running on {alias}? (`ssh {alias} herdr status`)",
                        local_socket.display()
                    )
                })
            }
        }
    }
}

fn forward_spec(local: &std::path::Path, remote: &str) -> String {
    format!("{}:{}", local.display(), remote)
}

pub fn check_socket_path(path: &std::path::Path) -> Result<()> {
    let len = path.as_os_str().len();
    if len > MAX_SOCKET_PATH_BYTES {
        bail!(
            "socket path {} is {len} bytes; Unix sockets allow {MAX_SOCKET_PATH_BYTES}. Set OPS_STATE_DIR to a shorter directory",
            path.display()
        );
    }
    Ok(())
}

async fn ensure_master(alias: &str, control_path: &std::path::Path) -> Result<()> {
    match ssh_control(control_path, &["-O", "check", alias]).await {
        Ok(()) => Ok(()),
        Err(err) => bail!(
            "no SSH master for {alias} at {} ({err}). Open one in your terminal, approve 1Password, then Ops reconnects on its own:\n  ssh -fNM -o ControlPath={} -o ControlPersist=8h {alias}",
            control_path.display(),
            control_path.display()
        ),
    }
}

/// Run `ssh -o ControlPath=<path> <args>`; `Ok` on exit 0.
async fn ssh_control(control_path: &std::path::Path, args: &[&str]) -> Result<()> {
    let control = format!("ControlPath={}", control_path.display());
    let output = Command::new("ssh")
        .arg("-o")
        .arg(&control)
        .arg("-o")
        .arg("BatchMode=yes")
        .args(args)
        .output()
        .await
        .context("cannot run ssh — is OpenSSH installed?")?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    bail!("ssh {} exited {}: {stderr}", args.join(" "), output.status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_overlong_local_socket_path() {
        let long = PathBuf::from(format!("/{}/ds9.sock", "x".repeat(120)));
        let err = Transport::ssh("ds9".into(), "/r.sock".into(), "/cm".into(), long).unwrap_err();
        assert!(err.to_string().contains("OPS_STATE_DIR"), "{err}");
    }

    #[test]
    fn accepts_short_local_socket_path() {
        let transport = Transport::ssh(
            "ds9".into(),
            "/Users/v/.config/herdr/herdr.sock".into(),
            "/Users/v/.ssh/cm-ds9".into(),
            "/Users/v/.local/state/ops/ds9.sock".into(),
        )
        .unwrap();
        assert_eq!(
            transport.describe(),
            "ssh ds9:/Users/v/.config/herdr/herdr.sock"
        );
    }

    #[test]
    fn forward_spec_is_local_colon_remote() {
        assert_eq!(
            forward_spec(std::path::Path::new("/l.sock"), "/r.sock"),
            "/l.sock:/r.sock"
        );
    }

    #[tokio::test]
    async fn local_connect_error_names_herdr_status() {
        let transport = Transport::Local {
            socket: PathBuf::from("/nonexistent/herdr.sock"),
        };
        let err = transport.connect().await.unwrap_err();
        assert!(format!("{err:#}").contains("herdr status"), "{err:#}");
    }

    #[tokio::test]
    async fn ssh_connect_without_master_tells_user_how_to_open_one() {
        let dir = std::env::temp_dir().join(format!("ops-t-{}", std::process::id()));
        let transport = Transport::ssh(
            "ops-test-nonexistent-alias".into(),
            "/r.sock".into(),
            dir.join("cm-none"),
            dir.join("h.sock"),
        )
        .unwrap();
        let err = transport.connect().await.unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("ssh -fNM"), "{text}");
        assert!(text.contains("ops-test-nonexistent-alias"), "{text}");
        let _ = std::fs::remove_dir_all(dir);
    }
}
