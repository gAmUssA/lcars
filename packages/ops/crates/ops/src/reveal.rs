//! Put a focused pane on screen: find the local Herdr TUI client attached to a
//! host's socket and reveal the iTerm2 session it runs in.
//!
//! Focusing a pane over the socket only moves Herdr's attached clients; the
//! window hosting them may sit behind others or on another Space. Ported from
//! herdr-notify's `focus.py`: list `herdr` processes with a tty, read each
//! one's argv and a fixed allowlist of environment variables
//! (`KERN_PROCARGS2`), resolve which socket it is attached to the way Herdr
//! does, and open `iterm2:reveal?sessionid=…`. That URL needs no Automation
//! consent and takes ~20 ms, against an osascript round-trip.
//!
//! Local hosts only: a client of a remote Herdr runs `ssh`/`mosh` here and has
//! no socket to match, so callers fall back to raising the terminal app.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

/// Environment variables read from a client process. Everything else in a
/// shell's environment (tokens, keys) is discarded unread.
const ENV_ALLOWLIST: &[&str] = &[
    "HOME",
    "XDG_CONFIG_HOME",
    "HERDR_SESSION",
    "HERDR_SOCKET_PATH",
    "HERDR_CLIENT_SOCKET_PATH",
    "ITERM_SESSION_ID",
    "TERM_PROGRAM",
];

/// Local hosts' Herdr sockets, by host name.
#[derive(Debug, Clone, Default)]
pub struct Revealer {
    sockets: HashMap<String, PathBuf>,
}

impl Revealer {
    pub fn new(sockets: HashMap<String, PathBuf>) -> Self {
        Revealer { sockets }
    }

    /// Reveal the iTerm session showing `host`'s Herdr. `Ok(false)` when there
    /// is nothing to reveal (remote host, no attached iTerm client); the
    /// caller then raises the terminal app instead.
    pub async fn reveal(&self, host: &str) -> Result<bool> {
        let Some(socket) = self.sockets.get(host).cloned() else {
            return Ok(false);
        };
        let sessions = tokio::task::spawn_blocking(move || iterm_sessions_for(&socket))
            .await
            .context("client lookup task failed")??;
        let Some(session) = sessions.first() else {
            return Ok(false);
        };
        let status = tokio::process::Command::new("/usr/bin/open")
            .arg(format!("iterm2:reveal?sessionid={session}"))
            .status()
            .await
            .context("cannot run /usr/bin/open")?;
        if !status.success() {
            bail!("open iterm2:reveal for {session} exited with {status}");
        }
        Ok(true)
    }
}

/// iTerm session ids of local Herdr TUI clients attached to `socket`.
fn iterm_sessions_for(socket: &Path) -> Result<Vec<String>> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,tty=,comm="])
        .output()
        .context("cannot run /bin/ps")?;
    let target = canonical(socket);
    let mut sessions = Vec::new();
    for (pid, _tty) in herdr_processes(&String::from_utf8_lossy(&output.stdout)) {
        // The process may have exited since ps; never guess its session.
        let Ok((args, env)) = process_context(pid) else {
            continue;
        };
        let Some(attached) = client_socket(&args, &env) else {
            continue;
        };
        if canonical(&attached) != target {
            continue;
        }
        if let Some(session) = iterm_session_id(&env) {
            if !sessions.contains(&session) {
                sessions.push(session);
            }
        }
    }
    Ok(sessions)
}

fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// `(pid, tty)` of every `herdr` process with a controlling terminal, from
/// `ps -axo pid=,tty=,comm=`. Servers and hooks have no tty (`??`).
pub fn herdr_processes(ps: &str) -> Vec<(i32, String)> {
    ps.lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let tty = fields.next()?;
            let command: Vec<&str> = fields.collect();
            let command = command.join(" ");
            let name = Path::new(&command).file_name()?.to_str()?;
            (tty != "??" && name == "herdr").then(|| (pid, tty.to_string()))
        })
        .collect()
}

/// Decode a `KERN_PROCARGS2` buffer into argv and the allowlisted environment.
pub fn decode_procargs(data: &[u8]) -> Result<(Vec<String>, HashMap<String, String>)> {
    if data.len() < 4 {
        bail!("procargs too short");
    }
    let argc = i32::from_ne_bytes(data[..4].try_into().unwrap()).max(0) as usize;
    let rest = &data[4..];
    // Executable path, then NUL padding, then argv and envp.
    let exec_end = rest
        .iter()
        .position(|&b| b == 0)
        .context("procargs has no executable path")?;
    let after = &rest[exec_end..];
    let start = after.iter().position(|&b| b != 0).unwrap_or(after.len());
    let values: Vec<&[u8]> = after[start..].split(|&b| b == 0).collect();
    let args = values
        .iter()
        .take(argc)
        .map(|v| String::from_utf8_lossy(v).into_owned())
        .collect();
    let mut env = HashMap::new();
    for value in values.iter().skip(argc) {
        let text = String::from_utf8_lossy(value);
        if let Some((key, val)) = text.split_once('=') {
            if ENV_ALLOWLIST.contains(&key) {
                env.insert(key.to_string(), val.to_string());
            }
        }
    }
    Ok((args, env))
}

#[cfg(target_os = "macos")]
fn process_context(pid: i32) -> Result<(Vec<String>, HashMap<String, String>)> {
    // SAFETY: sysctl writes at most `size` bytes into `buffer` and updates
    // `size` to the length written.
    unsafe {
        let mut size = libc::sysconf(libc::_SC_ARG_MAX).max(4096) as libc::size_t;
        let mut buffer = vec![0u8; size];
        let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid];
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            buffer.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            bail!(
                "cannot read process {pid}: {}",
                std::io::Error::last_os_error()
            );
        }
        buffer.truncate(size);
        decode_procargs(&buffer)
    }
}

#[cfg(not(target_os = "macos"))]
fn process_context(_pid: i32) -> Result<(Vec<String>, HashMap<String, String>)> {
    bail!("revealing a terminal session is macOS-only")
}

/// The API socket a Herdr invocation is attached to, following Herdr's
/// session precedence; `None` for anything but a local TUI client (server,
/// CLI queries, `--remote`, unknown options).
pub fn client_socket(args: &[String], env: &HashMap<String, String>) -> Option<PathBuf> {
    let mut remaining = Vec::new();
    let mut session: Option<String> = None;
    let mut iter = args.iter().skip(1);
    while let Some(arg) = iter.next() {
        if arg == "--session" {
            session = Some(iter.next()?.clone());
        } else if let Some(value) = arg.strip_prefix("--session=") {
            session = Some(value.to_string());
        } else if arg != "--handoff" {
            remaining.push(arg.as_str());
        }
    }
    if remaining.len() == 3 && remaining[..2] == ["session", "attach"] {
        session = Some(remaining[2].to_string());
    } else if !remaining.is_empty() {
        return None;
    }
    let mut config = match env.get("XDG_CONFIG_HOME") {
        Some(xdg) => PathBuf::from(xdg),
        None => PathBuf::from(env.get("HOME").map(String::as_str).unwrap_or("~")).join(".config"),
    }
    .join("herdr");
    let session = match session {
        Some(session) => session,
        None => {
            if let Some(path) = env.get("HERDR_SOCKET_PATH") {
                return Some(PathBuf::from(path));
            }
            if env.contains_key("HERDR_CLIENT_SOCKET_PATH") {
                // A legacy client-only override cannot identify its API socket.
                return None;
            }
            env.get("HERDR_SESSION")
                .cloned()
                .unwrap_or_else(|| "default".to_string())
        }
    };
    if session != "default" {
        config = config.join("sessions").join(session);
    }
    Some(config.join("herdr.sock"))
}

/// The client's iTerm session, when the client really runs in iTerm.
///
/// A client started from an iTerm shell and re-exec'd elsewhere still
/// inherits `ITERM_SESSION_ID`; `TERM_PROGRAM` is rewritten by the terminal
/// that owns the tty, so it decides. The id goes into a URL handed to `open`,
/// so only iTerm's own alphabet (`wNtNpN:UUID`) passes.
pub fn iterm_session_id(env: &HashMap<String, String>) -> Option<String> {
    if let Some(term) = env.get("TERM_PROGRAM") {
        if term != "iTerm.app" {
            return None;
        }
    }
    let session = env.get("ITERM_SESSION_ID")?;
    let valid = !session.is_empty()
        && session
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == ':' || c == '-');
    valid.then(|| session.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn procargs(argv: &[&str], envp: &[&str]) -> Vec<u8> {
        let mut data = (argv.len() as i32).to_ne_bytes().to_vec();
        data.extend_from_slice(b"/usr/local/bin/herdr\0\0\0");
        for value in argv.iter().chain(envp) {
            data.extend_from_slice(value.as_bytes());
            data.push(0);
        }
        data
    }

    #[test]
    fn ps_keeps_only_herdr_clients_with_a_tty() {
        let ps = "  101 ttys003  /Users/me/.local/bin/herdr\n  102 ??       /Users/me/.local/bin/herdr\n  103 ttys004  -zsh\n  104 ttys005  /opt/herdr-tools/herdr-notify\n";
        assert_eq!(herdr_processes(ps), vec![(101, "ttys003".to_string())]);
    }

    #[test]
    fn procargs_keeps_argv_and_only_allowlisted_env() {
        let data = procargs(
            &["herdr", "--session", "work"],
            &[
                "HOME=/Users/me",
                "GITHUB_TOKEN=secret",
                "ITERM_SESSION_ID=w0t1p0:AB-12",
            ],
        );
        let (argv, env) = decode_procargs(&data).unwrap();
        assert_eq!(argv, args(&["herdr", "--session", "work"]));
        assert_eq!(env.get("HOME").map(String::as_str), Some("/Users/me"));
        assert_eq!(
            env.get("ITERM_SESSION_ID").map(String::as_str),
            Some("w0t1p0:AB-12")
        );
        assert!(!env.contains_key("GITHUB_TOKEN"));
    }

    #[test]
    fn bare_client_attaches_to_the_default_session() {
        let socket = client_socket(&args(&["herdr"]), &env(&[("HOME", "/h")]));
        assert_eq!(socket, Some(PathBuf::from("/h/.config/herdr/herdr.sock")));
    }

    #[test]
    fn named_sessions_resolve_under_sessions_dir() {
        let e = env(&[("HOME", "/h")]);
        let want = Some(PathBuf::from("/h/.config/herdr/sessions/work/herdr.sock"));
        assert_eq!(
            client_socket(&args(&["herdr", "--session", "work"]), &e),
            want
        );
        assert_eq!(client_socket(&args(&["herdr", "--session=work"]), &e), want);
        assert_eq!(
            client_socket(&args(&["herdr", "session", "attach", "work"]), &e),
            want
        );
        let from_env = env(&[("HOME", "/h"), ("HERDR_SESSION", "work")]);
        assert_eq!(client_socket(&args(&["herdr"]), &from_env), want);
    }

    #[test]
    fn socket_env_and_xdg_are_honoured() {
        let e = env(&[("HERDR_SOCKET_PATH", "/tmp/x.sock")]);
        assert_eq!(
            client_socket(&args(&["herdr"]), &e),
            Some(PathBuf::from("/tmp/x.sock"))
        );
        let e = env(&[("HOME", "/h"), ("XDG_CONFIG_HOME", "/cfg")]);
        assert_eq!(
            client_socket(&args(&["herdr", "--handoff"]), &e),
            Some(PathBuf::from("/cfg/herdr/herdr.sock"))
        );
        let e = env(&[("HERDR_CLIENT_SOCKET_PATH", "/c.sock")]);
        assert_eq!(client_socket(&args(&["herdr"]), &e), None);
    }

    #[test]
    fn servers_queries_and_remote_clients_are_not_clients() {
        let e = env(&[("HOME", "/h")]);
        for argv in [
            &["herdr", "server"][..],
            &["herdr", "pane", "list"],
            &["herdr", "--remote", "mini"],
            &["herdr", "--session"],
        ] {
            assert_eq!(client_socket(&args(argv), &e), None, "{argv:?}");
        }
    }

    #[test]
    fn iterm_session_requires_iterm_and_a_safe_id() {
        assert_eq!(
            iterm_session_id(&env(&[("ITERM_SESSION_ID", "w0t1p0:AB-12")])),
            Some("w0t1p0:AB-12".to_string())
        );
        assert_eq!(
            iterm_session_id(&env(&[
                ("ITERM_SESSION_ID", "w0t1p0:AB"),
                ("TERM_PROGRAM", "iTerm.app")
            ])),
            Some("w0t1p0:AB".to_string())
        );
        assert_eq!(
            iterm_session_id(&env(&[
                ("ITERM_SESSION_ID", "w0t1p0:AB"),
                ("TERM_PROGRAM", "ghostty")
            ])),
            None
        );
        assert_eq!(
            iterm_session_id(&env(&[("ITERM_SESSION_ID", "x&open=/bin/sh")])),
            None
        );
        assert_eq!(iterm_session_id(&env(&[])), None);
    }

    #[tokio::test]
    async fn remote_or_unknown_hosts_reveal_nothing() {
        let revealer = Revealer::new(HashMap::new());
        assert!(!revealer.reveal("mini").await.unwrap());
    }
}
