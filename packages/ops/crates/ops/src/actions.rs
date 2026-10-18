//! Relays plugin actions to the Herdr host that owns the session. Ops adds
//! no behaviour of its own here: a focus is Herdr's `agent.focus` or
//! `pane.focus`, nothing more.

use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};
use herdr_compat::api::schema::{
    AgentPromptParams, AgentSendKeysParams, AgentTarget, Method, PaneScrollParams,
    PaneSendKeysParams, PaneTarget, ResponseResult,
};

use crate::herdr::client::request_once;
use crate::herdr::transport::Transport;
use crate::model::Session;

pub struct ActionRelay {
    hosts: HashMap<String, Transport>,
}

impl ActionRelay {
    pub fn new(hosts: HashMap<String, Transport>) -> Self {
        ActionRelay { hosts }
    }

    /// Focus the pane behind `session` on its host. Panes with a recognised
    /// agent go through `agent.focus` (which also marks the agent seen);
    /// plain shells through `pane.focus`.
    pub async fn focus(&self, session: &Session) -> Result<()> {
        let transport = self.transport_for(session)?;
        let method = if session.agent.is_some() {
            Method::AgentFocus(AgentTarget {
                target: session.pane_id.clone(),
            })
        } else {
            Method::PaneFocus(PaneTarget {
                pane_id: session.pane_id.clone(),
            })
        };
        let id = format!("ops:{}:focus:{}", session.host, session.pane_id);
        // Observed on Herdr 0.9.4: `agent.focus` answers with the focused
        // agent's `agent_info`, `pane.focus` with `ok`; accept either.
        match request_once(transport, id, method).await? {
            ResponseResult::Ok {}
            | ResponseResult::AgentInfo { .. }
            | ResponseResult::PaneInfo { .. } => Ok(()),
            other => bail!("focus on {} returned {other:?}", session.id),
        }
    }

    /// Scroll the pane's terminal by `delta` rows (positive = up into
    /// scrollback). Herdr's `pane.scroll` takes an absolute offset, so read
    /// the current one first and clamp. Returns the offset applied.
    pub async fn scroll(&self, session: &Session, delta: i64) -> Result<u64> {
        let transport = self.transport_for(session)?;
        let pane_id = session.pane_id.clone();
        let id = format!("ops:{}:scroll:{}", session.host, pane_id);
        let scroll = match request_once(
            transport,
            format!("{id}:get"),
            Method::PaneGet(PaneTarget {
                pane_id: pane_id.clone(),
            }),
        )
        .await?
        {
            ResponseResult::PaneInfo { pane } => pane.scroll,
            other => bail!("pane.get for {} returned {other:?}", session.id),
        };
        let Some(scroll) = scroll else {
            bail!(
                "pane {} reports no scroll state; nothing to scroll",
                session.id
            );
        };
        let target = (scroll.offset_from_bottom as i64 + delta)
            .clamp(0, scroll.max_offset_from_bottom as i64) as u64;
        if target == scroll.offset_from_bottom {
            return Ok(target);
        }
        match request_once(
            transport,
            id,
            Method::PaneScroll(PaneScrollParams {
                pane_id,
                offset_from_bottom: target,
            }),
        )
        .await?
        {
            ResponseResult::Ok {} | ResponseResult::PaneInfo { .. } => Ok(target),
            other => bail!("pane.scroll on {} returned {other:?}", session.id),
        }
    }

    /// Submit `text` plus Enter to the agent in the pane (Herdr `agent.prompt`,
    /// no wait). Herdr refuses with `agent_blocked` while an approval dialog is
    /// open, which is the behaviour Conn relies on.
    pub async fn prompt(&self, session: &Session, text: &str) -> Result<()> {
        if session.agent.is_none() {
            bail!("{} has no agent to prompt", session.id);
        }
        if text.trim().is_empty() {
            bail!("refusing to send an empty prompt to {}", session.id);
        }
        let transport = self.transport_for(session)?;
        let id = format!("ops:{}:prompt:{}", session.host, session.pane_id);
        match request_once(
            transport,
            id,
            Method::AgentPrompt(AgentPromptParams {
                target: session.pane_id.clone(),
                text: text.to_string(),
                wait: None,
            }),
        )
        .await?
        {
            ResponseResult::Ok {} | ResponseResult::AgentPrompted { .. } => Ok(()),
            other => bail!("agent.prompt on {} returned {other:?}", session.id),
        }
    }

    /// Send logical key presses (`enter`, `esc`, `y`, `ctrl+c`, …) to the pane.
    /// Herdr validates the key names before writing anything. Which keys mean
    /// "approve" is the plugin's decision (Conn), not Ops's.
    pub async fn send_keys(&self, session: &Session, keys: &[String]) -> Result<()> {
        if keys.is_empty() {
            bail!("send_keys for {} needs at least one key", session.id);
        }
        let transport = self.transport_for(session)?;
        let id = format!("ops:{}:keys:{}", session.host, session.pane_id);
        let method = if session.agent.is_some() {
            Method::AgentSendKeys(AgentSendKeysParams {
                target: session.pane_id.clone(),
                keys: keys.to_vec(),
            })
        } else {
            Method::PaneSendKeys(PaneSendKeysParams {
                pane_id: session.pane_id.clone(),
                keys: keys.to_vec(),
            })
        };
        match request_once(transport, id, method).await? {
            ResponseResult::Ok {}
            | ResponseResult::AgentInfo { .. }
            | ResponseResult::PaneInfo { .. } => Ok(()),
            other => bail!("send_keys on {} returned {other:?}", session.id),
        }
    }

    fn transport_for(&self, session: &Session) -> Result<&Transport> {
        self.hosts.get(&session.host).ok_or_else(|| {
            anyhow!(
                "host `{}` is not configured in ops.toml — the session list is stale",
                session.host
            )
        })
    }
}
