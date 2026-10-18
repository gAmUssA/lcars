---
# lcars-ipvd
title: 'Workspace Key: one key per Herdr workspace, press focuses it'
status: completed
type: feature
priority: normal
created_at: 2026-10-18T20:27:42Z
updated_at: 2026-10-18T20:33:55Z
parent: lcars-bv6p
---

Ops: FleetView gains workspaces (host, id, label, cwd, focused) from the snapshot, refreshed on workspace.* events; action focus_workspace relays workspace.focus. Plugin: WorkspaceKeyAction with a slot index, colour by the worst agent state inside, agent count, label; press focuses the workspace and raises iTerm2.

## Summary of Changes

Ops: Workspace model from the snapshot's WorkspaceInfo (label, number, focused, pane_count, aggregate state), HostEvent::Workspaces replaces a host's set, workspace.* broadcast subscriptions trigger a fresh snapshot, FleetView.workspaces sorted by host then number, action focus_workspace → workspace.focus (tests in model/client/server). Plugin: WorkspaceKeyAction with slot setting, renderWorkspaceKey (state colour, label, agent count, host tag, focus ring, dim when down), store.workspaceAt, token-aware ops.focusWorkspace. Verified live: five workspaces across both hosts in /sessions, focus_workspace relayed.
