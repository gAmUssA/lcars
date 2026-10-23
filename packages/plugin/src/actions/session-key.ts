// Session Key: one Herdr session per key (a Herdr server, local or on another
// machine). Colour is its worst agent state, the subtitle counts agents or
// blocked ones, the corner names the machine. Tap filters the agent keys and
// Workspace Keys to the session (tap again for all); long press turns the
// wall page on this deck. Blocked agents elsewhere stay on the wall.
import streamDeck, {
  SingletonAction,
  type DidReceiveSettingsEvent,
  type KeyDownEvent,
  type KeyUpEvent,
  type WillAppearEvent,
  type WillDisappearEvent,
} from "@elgato/streamdeck";
import { renderSessionKey } from "../core/render";
import type { FleetStore } from "../core/store";
import { summariseSessions, type SessionSummary } from "../core/view-state";
import { LONG_PRESS_MS } from "./inbox-key";

export const SESSION_KEY_UUID = "io.gamov.lcars.session";

export type SessionSettings = { slotIndex?: number | string };

export function sessionSlot(settings: SessionSettings): number {
  const parsed = Number(settings.slotIndex ?? 0);
  return Number.isFinite(parsed) && parsed >= 0 ? Math.floor(parsed) : 0;
}

/** Session at a slot, in the order Ops lists sessions, counted over every agent. */
export function sessionAt(store: FleetStore, slot: number): SessionSummary | null {
  const state = store.getState();
  if (state.view === null) return null;
  return summariseSessions(state.view, store.allKeys())[slot] ?? null;
}

export class SessionKeyAction extends SingletonAction<SessionSettings> {
  override readonly manifestId = SESSION_KEY_UUID;
  readonly #slots = new Map<string, { slot: number; device: string }>();
  readonly #pressedAt = new Map<string, number>();

  constructor(
    private readonly store: FleetStore,
    /** Wall keys on a deck, for the page read-out and the long press. */
    private readonly wallSize: (device: string) => number,
    private readonly now: () => number = Date.now,
  ) {
    super();
  }

  override onWillAppear(ev: WillAppearEvent<SessionSettings>): void {
    this.#slots.set(ev.action.id, { slot: sessionSlot(ev.payload.settings), device: ev.action.device.id });
    this.renderAll();
  }

  override onDidReceiveSettings(ev: DidReceiveSettingsEvent<SessionSettings>): void {
    this.#slots.set(ev.action.id, { slot: sessionSlot(ev.payload.settings), device: ev.action.device.id });
    this.renderAll();
  }

  override onWillDisappear(ev: WillDisappearEvent<SessionSettings>): void {
    this.#slots.delete(ev.action.id);
    this.#pressedAt.delete(ev.action.id);
  }

  override onKeyDown(ev: KeyDownEvent<SessionSettings>): void {
    this.#pressedAt.set(ev.action.id, this.now());
  }

  override async onKeyUp(ev: KeyUpEvent<SessionSettings>): Promise<void> {
    const start = this.#pressedAt.get(ev.action.id);
    this.#pressedAt.delete(ev.action.id);
    if (start !== undefined && this.now() - start >= LONG_PRESS_MS) {
      this.store.nextWallPage();
      const { page, pages } = this.store.wallPage(this.wallSize(ev.action.device.id));
      streamDeck.logger.info(`wall page ${page + 1}/${pages}`);
      return;
    }
    const session = sessionAt(this.store, sessionSlot(ev.payload.settings));
    if (!session) {
      await ev.action.showAlert();
      return;
    }
    this.store.toggleSessionFilter(session.name);
  }

  renderAll(): void {
    const state = this.store.getState();
    this.actions.forEach((a) => {
      if (!a.isKey()) return;
      const placed = this.#slots.get(a.id) ?? { slot: 0, device: a.device.id };
      const session = sessionAt(this.store, placed.slot);
      void a.setTitle("");
      if (!session) {
        void a.setImage(renderSessionKey(null));
        return;
      }
      const { page, pages } = this.store.wallPage(this.wallSize(placed.device));
      const agents = Object.values(session.counts).reduce((n, c) => n + c, 0);
      void a.setImage(
        renderSessionKey({
          name: session.name,
          machine: session.session?.machine ?? null,
          worst: session.worst,
          agents,
          blocked: session.counts.blocked,
          connected: session.session?.connected !== false,
          selected: state.ui.sessionFilter === session.name,
          page: pages > 1 ? `${page + 1}/${pages}` : null,
          live: state.healthy,
        }),
      );
    });
  }
}
