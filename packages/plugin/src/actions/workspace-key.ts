// Workspace Key: one Herdr workspace per key. Colour is Herdr's roll-up of
// the agents inside, the label is the workspace name, the subtitle counts
// agents and names the host. Press focuses the workspace and raises iTerm2.
import streamDeck, {
  SingletonAction,
  type DidReceiveSettingsEvent,
  type KeyDownEvent,
  type WillAppearEvent,
  type WillDisappearEvent,
} from "@elgato/streamdeck";
import { renderWorkspaceKey } from "../core/render";
import type { FleetStore } from "../core/store";
import type { OpsActions } from "../ops/actions";
import type { TerminalActivator } from "../os/terminal";
import type { Workspace } from "../ops/types";

export const WORKSPACE_KEY_UUID = "io.gamov.lcars.workspace";

export type WorkspaceSettings = { slotIndex?: number | string };

export function workspaceSlot(settings: WorkspaceSettings): number {
  const parsed = Number(settings.slotIndex ?? 0);
  return Number.isFinite(parsed) && parsed >= 0 ? Math.floor(parsed) : 0;
}

/** Agents (sessions with an agent) inside a workspace, from the current view. */
export function agentsIn(store: FleetStore, workspace: Workspace): number {
  return store.allKeys().filter((s) => s.host === workspace.host && s.workspace_id === workspace.workspace_id).length;
}

export class WorkspaceKeyAction extends SingletonAction<WorkspaceSettings> {
  override readonly manifestId = WORKSPACE_KEY_UUID;
  readonly #slots = new Map<string, number>();

  constructor(
    private readonly store: FleetStore,
    private readonly ops: OpsActions,
    private readonly terminal: TerminalActivator,
  ) {
    super();
  }

  override onWillAppear(ev: WillAppearEvent<WorkspaceSettings>): void {
    this.#slots.set(ev.action.id, workspaceSlot(ev.payload.settings));
    this.renderAll();
  }

  override onDidReceiveSettings(ev: DidReceiveSettingsEvent<WorkspaceSettings>): void {
    this.#slots.set(ev.action.id, workspaceSlot(ev.payload.settings));
    this.renderAll();
  }

  override onWillDisappear(ev: WillDisappearEvent<WorkspaceSettings>): void {
    this.#slots.delete(ev.action.id);
  }

  override async onKeyDown(ev: KeyDownEvent<WorkspaceSettings>): Promise<void> {
    const workspace = this.store.workspaceAt(workspaceSlot(ev.payload.settings));
    if (!workspace) return;
    try {
      await this.ops.focusWorkspace(workspace.id);
      await this.terminal.activate();
    } catch (e) {
      streamDeck.logger.error(`focus workspace ${workspace.id} failed: ${String(e)}`);
      await ev.action.showAlert();
    }
  }

  renderAll(): void {
    const state = this.store.getState();
    const multiHost = (state.view?.hosts.length ?? 0) > 1;
    this.actions.forEach((a) => {
      if (!a.isKey()) return;
      const slot = this.#slots.get(a.id) ?? 0;
      const workspace = this.store.workspaceAt(slot);
      void a.setTitle("");
      if (!workspace) {
        void a.setImage(renderWorkspaceKey(null));
        return;
      }
      const hostUp = state.view?.hosts.find((h) => h.name === workspace.host)?.connected !== false;
      void a.setImage(
        renderWorkspaceKey({
          label: workspace.label,
          state: workspace.state,
          agents: agentsIn(this.store, workspace),
          host: multiHost ? workspace.host.slice(0, 4).toUpperCase() : null,
          focused: workspace.focused,
          live: state.healthy && hostUp,
        }),
      );
    });
  }
}
