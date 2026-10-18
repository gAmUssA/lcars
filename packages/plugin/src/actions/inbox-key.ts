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
import { hostTag, labelFor, type DisplayMode } from "../core/labels";
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

export function slotConfig(settings: InboxSettings): SlotConfig {
  const parsed = Number(settings.slotIndex ?? 0);
  return {
    slot: Number.isFinite(parsed) && parsed >= 0 ? Math.floor(parsed) : 0,
    display: settings.display === "project" ? "project" : "title",
    // "paged" follows the Plus scroll dial (8 per page); "wall" is an absolute
    // slot over the whole list, for the XL where every agent has its own key.
    layout: settings.layout === "wall" ? "wall" : "paged",
  };
}

function sessionFor(store: FleetStore, config: SlotConfig) {
  return config.layout === "wall" ? store.sessionAtAbsolute(config.slot) : store.sessionAt(config.slot);
}

export const INBOX_KEY_UUID = "io.gamov.lcars.inbox";

// Plain subclass instead of the SDK's `@action` decorator: it keeps the
// source free of TC39 decorators so tsc, vitest and rollup agree on syntax.
export class InboxKeyAction extends SingletonAction<InboxSettings> {
  override readonly manifestId = INBOX_KEY_UUID;
  readonly #configs = new Map<string, SlotConfig>();
  readonly #pressStart = new Map<string, number>();

  constructor(
    private readonly store: FleetStore,
    private readonly ops: OpsActions,
    private readonly terminal: TerminalActivator,
    private readonly now: () => number = Date.now,
  ) {
    super();
  }

  override onWillAppear(ev: WillAppearEvent<InboxSettings>): void {
    this.#configs.set(ev.action.id, slotConfig(ev.payload.settings));
    this.renderAll();
  }

  override onDidReceiveSettings(ev: DidReceiveSettingsEvent<InboxSettings>): void {
    this.#configs.set(ev.action.id, slotConfig(ev.payload.settings));
    this.renderAll();
  }

  override onWillDisappear(ev: WillDisappearEvent<InboxSettings>): void {
    this.#configs.delete(ev.action.id);
    this.#pressStart.delete(ev.action.id);
  }

  override onKeyDown(ev: KeyDownEvent<InboxSettings>): void {
    this.#pressStart.set(ev.action.id, this.now());
  }

  override async onKeyUp(ev: KeyUpEvent<InboxSettings>): Promise<void> {
    const start = this.#pressStart.get(ev.action.id);
    this.#pressStart.delete(ev.action.id);
    const session = sessionFor(this.store, slotConfig(ev.payload.settings));
    if (!session) return;
    const longPress = start !== undefined && this.now() - start >= LONG_PRESS_MS;
    if (longPress) {
      await this.conn(session, ev.action);
      return;
    }
    try {
      await this.ops.focus(session.id);
    } catch (e) {
      streamDeck.logger.error(`focus ${session.id} failed: ${String(e)}`);
      await ev.action.showAlert();
      return;
    }
    // Raise the terminal so the focused pane is on screen; a raise failure
    // must not mask the successful focus.
    try {
      await this.terminal.activate();
    } catch (e) {
      streamDeck.logger.error(`raise terminal failed: ${String(e)}`);
    }
  }

  /** Conn: confirm a blocked agent's dialog. Anything else is a refused hold. */
  async conn(session: { id: string; state: string }, action: { showAlert(): Promise<void>; showOk(): Promise<void> }): Promise<void> {
    if (session.state !== "blocked") {
      streamDeck.logger.info(`conn on ${session.id} ignored: state is ${session.state}, not blocked`);
      await action.showAlert();
      return;
    }
    try {
      await this.ops.sendKeys(session.id, CONN_KEYS);
      streamDeck.logger.info(`conn: confirmed ${session.id}`);
      await action.showOk();
    } catch (e) {
      streamDeck.logger.error(`conn on ${session.id} failed: ${String(e)}`);
      await action.showAlert();
    }
  }

  /** Blink the key that currently shows `sessionId`, if any. */
  flash(sessionId: string): void {
    this.actions.forEach((a) => {
      if (!a.isKey()) return;
      const config = this.#configs.get(a.id);
      if (!config) return;
      if (sessionFor(this.store, config)?.id === sessionId) void a.showAlert();
    });
  }

  renderAll(): void {
    const state = this.store.getState();
    const neverConnected = state.view === null;
    this.actions.forEach((a) => {
      if (!a.isKey()) return;
      const config = this.#configs.get(a.id) ?? { slot: 0, display: "title" as DisplayMode, layout: "paged" as KeyLayout };
      void a.setTitle("");
      if (neverConnected) {
        void a.setImage(renderOfflineSvg());
        return;
      }
      const session = sessionFor(this.store, config);
      void a.setImage(
        renderKeySvg(
          session
            ? {
                label: labelFor(session, state.keys, config.display),
                status: session.state,
                agent: session.agent ?? "shell",
                host: hostTag(session, state.view),
                live: state.healthy,
              }
            : null,
        ),
      );
    });
  }
}
