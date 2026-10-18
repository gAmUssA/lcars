//! Relays plugin actions to the Herdr host that owns the session. Ops adds
//! no behaviour of its own here: a focus is Herdr's `agent.focus` or
//! `pane.focus`, nothing more.

use std::collections::HashMap;

use anyhow::{anyhow, bail, Result};
use herdr_compat::api::schema::{AgentTarget, Method, PaneTarget, ResponseResult};

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
        let transport = self.hosts.get(&session.host).ok_or_else(|| {
            anyhow!(
                "host `{}` is not configured in ops.toml — the session list is stale",
                session.host
            )
        })?;
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
}
