// Inbox Key: one Herdr agent pane per key. Colour is the agent state, short
// tap focuses the pane (via Ops) and raises the terminal. Long press is Conn:
// on a blocked pane it confirms the agent's highlighted option (Enter); on
// any other pane it does nothing but blink. Conn never fires on a short tap.
import streamDeck, {
  SingletonAction,
  type DidReceiveSettingsEvent,
  type KeyDownEvent,
  type KeyUpEvent,
  type WillAppearEvent,
  type WillDisappearEvent,
} from "@elgato/streamdeck";
import type { FleetStore } from "../core/store";
import { sessionTag, labelFor, type DisplayMode } from "../core/labels";
import { renderKeySvg, renderOfflineSvg } from "../core/render";
import type { OpsActions } from "../ops/actions";
import type { TerminalActivator } from "../os/terminal";

export type KeyLayout = "paged" | "wall";
export type InboxSettings = { slotIndex?: number | string; display?: DisplayMode; layout?: KeyLayout };

/** Presses at or above this are long presses (Conn). */
export const LONG_PRESS_MS = 400;

/** What Conn sends: confirm whatever the agent's dialog has highlighted. */
export const CONN_KEYS = ["enter"];

type SlotConfig = { slot: number; display: DisplayMode; layout: KeyLayout };
type PlacedConfig = SlotConfig & { device: string };

export function slotConfig(settings: InboxSettings): SlotConfig {
  const parsed = Number(settings.slotIndex ?? 0);
  return {
    slot: Number.isFinite(parsed) && parsed >= 0 ? Math.floor(parsed) : 0,
    display: settings.display === "project" ? "project" : "title",
    // "paged" follows the Plus scroll dial (one page per paged key set); "wall" is an absolute
    // slot over the whole list, for the XL where every agent has its own key.
    layout: settings.layout === "wall" ? "wall" : "paged",
  };
}

/** Wall size of one deck: the highest wall slot on it, plus one. */
export function wallSizeOf(configs: Iterable<{ slot: number; layout: KeyLayout; device: string }>, device: string): number {
  let size = 0;
  for (const c of configs) if (c.device === device && c.layout === "wall") size = Math.max(size, c.slot + 1);
  return size;
}

/**
 * Paged keys across the decks: the highest paged slot plus one, or 0 when
 * none is placed. The Plus pages agents by it, so a page that gives up a key
 * (for a Rooms key, say) still shows every agent.
 */
export function pagedSizeOf(configs: Iterable<{ slot: number; layout: KeyLayout }>): number {
  let size = 0;
  for (const c of configs) if (c.layout === "paged") size = Math.max(size, c.slot + 1);
  return size;
}

export const INBOX_KEY_UUID = "io.gamov.lcars.inbox";

// Plain subclass instead of the SDK's `@action` decorator: it keeps the
// source free of TC39 decorators so tsc, vitest and rollup agree on syntax.
export class InboxKeyAction extends SingletonAction<InboxSettings> {
  override readonly manifestId = INBOX_KEY_UUID;
  readonly #configs = new Map<string, PlacedConfig>();
  // What was under the finger when it went down: the deck can reorder
  // during a hold, and Conn must confirm the agent the operator saw.
  readonly #press = new Map<string, { at: number; paneId: string | null }>();

  constructor(
    private readonly store: FleetStore,
    private readonly ops: OpsActions,
    private readonly terminal: TerminalActivator,
    private readonly now: () => number = Date.now,
  ) {
    super();
  }

  override onWillAppear(ev: WillAppearEvent<InboxSettings>): void {
    this.#configs.set(ev.action.id, { ...slotConfig(ev.payload.settings), device: ev.action.device.id });
    this.store.setPageSize(pagedSizeOf(this.#configs.values()));
    this.renderAll();
  }

  override onDidReceiveSettings(ev: DidReceiveSettingsEvent<InboxSettings>): void {
    this.#configs.set(ev.action.id, { ...slotConfig(ev.payload.settings), device: ev.action.device.id });
    this.store.setPageSize(pagedSizeOf(this.#configs.values()));
    this.renderAll();
  }

  /** Wall keys on one deck; the Session Key pages by it. */
  wallSize(device: string): number {
    return wallSizeOf(this.#configs.values(), device);
  }

  paneFor(config: PlacedConfig) {
    return config.layout === "wall" ? this.store.paneOnWall(config.slot, this.wallSize(config.device)) : this.store.paneAt(config.slot);
  }

  override onWillDisappear(ev: WillDisappearEvent<InboxSettings>): void {
    this.#configs.delete(ev.action.id);
    this.#press.delete(ev.action.id);
  }

  override onKeyDown(ev: KeyDownEvent<InboxSettings>): void {
    const shown = this.paneFor({ ...slotConfig(ev.payload.settings), device: ev.action.device.id });
    this.#press.set(ev.action.id, { at: this.now(), paneId: shown?.id ?? null });
  }

  override async onKeyUp(ev: KeyUpEvent<InboxSettings>): Promise<void> {
    const press = this.#press.get(ev.action.id);
    this.#press.delete(ev.action.id);
    const pane = this.resolvePress(press?.paneId ?? null);
    if (!pane) {
      if (press?.paneId) await ev.action.showAlert(); // it moved or vanished during the hold
      return;
    }
    const start = press?.at;
    const longPress = start !== undefined && this.now() - start >= LONG_PRESS_MS;
    if (longPress) {
      await this.conn(pane, ev.action);
      return;
    }
    let revealed: boolean;
    try {
      revealed = await this.ops.focus(pane.id);
    } catch (e) {
      streamDeck.logger.error(`focus ${pane.id} failed: ${String(e)}`);
      await ev.action.showAlert();
      return;
    }
    // Ops already put the pane's iTerm session on screen (local session).
    if (revealed) return;
    // Otherwise raise the terminal so the focused pane is on screen; a raise
    // failure must not mask the successful focus.
    try {
      await this.terminal.activate();
    } catch (e) {
      streamDeck.logger.error(`raise terminal failed: ${String(e)}`);
    }
  }

  /** The pane pressed at key down, looked up again now; null if it is gone. */
  resolvePress(paneId: string | null) {
    if (!paneId) return null;
    return this.store.allKeys().find((s) => s.id === paneId) ?? null;
  }

  /** Conn: confirm a blocked agent's dialog. Anything else is a refused hold. */
  async conn(pane: { id: string; state: string }, action: { showAlert(): Promise<void>; showOk(): Promise<void> }): Promise<void> {
    if (pane.state !== "blocked") {
      streamDeck.logger.info(`conn on ${pane.id} ignored: state is ${pane.state}, not blocked`);
      await action.showAlert();
      return;
    }
    try {
      await this.ops.sendKeys(pane.id, CONN_KEYS);
      streamDeck.logger.info(`conn: confirmed ${pane.id}`);
      await action.showOk();
    } catch (e) {
      streamDeck.logger.error(`conn on ${pane.id} failed: ${String(e)}`);
      await action.showAlert();
    }
  }

  /** Blink the key that currently shows `paneId`, if any. */
  flash(paneId: string): void {
    this.actions.forEach((a) => {
      if (!a.isKey()) return;
      const config = this.#configs.get(a.id);
      if (!config) return;
      if (this.paneFor(config)?.id === paneId) void a.showAlert();
    });
  }

  renderAll(): void {
    const state = this.store.getState();
    const neverConnected = state.view === null;
    this.actions.forEach((a) => {
      if (!a.isKey()) return;
      const config = this.#configs.get(a.id) ?? { slot: 0, display: "title" as DisplayMode, layout: "paged" as KeyLayout, device: a.device.id };
      void a.setTitle("");
      if (neverConnected) {
        void a.setImage(renderOfflineSvg());
        return;
      }
      const pane = this.paneFor(config);
      const sessionUp = pane ? state.view?.sessions.find((h) => h.name === pane.session)?.connected !== false : true;
      void a.setImage(
        renderKeySvg(
          pane
            ? {
                label: labelFor(pane, state.keys, config.display),
                status: pane.state,
                agent: pane.agent ?? "shell",
                session: sessionTag(pane, state.view),
                // Dim when Ops is gone or when this pane's session is: both
                // mean the colour is a memory, not a reading.
                live: state.healthy && sessionUp,
              }
            : null,
        ),
      );
    });
  }
}
