// The four Stream Deck + dials. Each dial action also owns the 200×100 touch
// strip segment above it; all four render slices of one shared scene.
//
//   dial 1  scroll   rotate pages the key list, press returns to page 1
//   dial 2  session  rotate cycles the session filter, press clears it (uuid dial.machine, kept for placed dials)
//   dial 3  effort   rotate picks a Claude Code effort level, press sends /effort <level>
//   dial 4  pane     rotate scrolls the focused pane (or the ticker, in ticker mode),
//                    press toggles the strip between fleet bar and inbox ticker
import streamDeck, {
  SingletonAction,
  type DialDownEvent,
  type DialRotateEvent,
  type TouchTapEvent,
  type WillAppearEvent,
} from "@elgato/streamdeck";
import type { FleetStore } from "../core/store";
import { renderStripSegment, stripTap, totalsOf, type StripScene } from "../core/strip";
import { blockedPanes, pageCount, summariseSessions } from "../core/view-state";
import type { OpsActions } from "../ops/actions";
import type { TerminalActivator } from "../os/terminal";

export const DIAL_UUIDS = {
  scroll: "io.gamov.lcars.dial.scroll",
  machine: "io.gamov.lcars.dial.machine",
  effort: "io.gamov.lcars.dial.effort",
  pane: "io.gamov.lcars.dial.pane",
} as const;

/** Terminal rows scrolled per dial tick. */
export const ROWS_PER_TICK = 3;

export type DialDeps = { store: FleetStore; ops: OpsActions; terminal: TerminalActivator };

/** Build the shared strip scene from the store. Exported for tests. */
export function sceneFrom(store: FleetStore): StripScene {
  const s = store.getState();
  return {
    mode: s.ui.stripMode,
    sessions: summariseSessions(s.view, store.allKeys()),
    sessionFilter: s.ui.sessionFilter,
    workspaceLabel: s.view?.workspaces.find((w) => w.id === s.ui.workspaceFilter)?.label ?? null,
    page: s.ui.page,
    pages: pageCount(s.keys.length, store.pageSize()),
    effort: s.ui.effort,
    blocked: blockedPanes(s.keys),
    tickerIndex: s.ui.tickerIndex,
    totals: totalsOf(s.keys),
    live: s.healthy,
  };
}

/** Scroll delta for a dial rotation: clockwise moves toward the bottom. */
export function scrollDelta(ticks: number): number {
  return -ticks * ROWS_PER_TICK;
}

abstract class StripDial extends SingletonAction {
  constructor(protected readonly deps: DialDeps) {
    super();
  }

  override onWillAppear(_ev: WillAppearEvent): void {
    this.renderAll();
  }

  override async onTouchTap(ev: TouchTapEvent): Promise<void> {
    const segment = ev.action.coordinates?.column ?? 0;
    const tap = stripTap(sceneFrom(this.deps.store), segment, ev.payload.tapPos[0]);
    switch (tap.kind) {
      case "session":
        this.deps.store.setSessionFilter(tap.session);
        break;
      case "clear-filter":
        this.deps.store.setSessionFilter(null);
        break;
      case "focus":
        await this.focus(tap.paneId);
        break;
      case "none":
        break;
    }
  }

  protected async focus(paneId: string): Promise<void> {
    try {
      if (!(await this.deps.ops.focus(paneId))) await this.deps.terminal.activate();
    } catch (e) {
      streamDeck.logger.error(`focus ${paneId} failed: ${String(e)}`);
    }
  }

  renderAll(): void {
    const scene = sceneFrom(this.deps.store);
    this.actions.forEach((a) => {
      if (!a.isDial()) return;
      const segment = a.coordinates?.column ?? 0;
      void a.setFeedback({ canvas: renderStripSegment(scene, segment) });
    });
  }
}

export class ScrollDial extends StripDial {
  override readonly manifestId = DIAL_UUIDS.scroll;
  override onDialRotate(ev: DialRotateEvent): void {
    this.deps.store.movePage(ev.payload.ticks);
  }
  override onDialDown(_ev: DialDownEvent): void {
    this.deps.store.resetPage();
  }
}

export class MachineDial extends StripDial {
  override readonly manifestId = DIAL_UUIDS.machine;
  override onDialRotate(ev: DialRotateEvent): void {
    this.deps.store.cycleSessionFilter(ev.payload.ticks);
  }
  override onDialDown(_ev: DialDownEvent): void {
    this.deps.store.setSessionFilter(null);
  }
}

export class EffortDial extends StripDial {
  override readonly manifestId = DIAL_UUIDS.effort;
  override onDialRotate(ev: DialRotateEvent): void {
    this.deps.store.cycleEffortLevel(ev.payload.ticks);
  }
  override async onDialDown(ev: DialDownEvent): Promise<void> {
    const target = this.deps.store.focusedPane();
    const effort = this.deps.store.getState().ui.effort;
    if (!target || target.agent !== "claude") {
      streamDeck.logger.info(`effort: focused pane is ${target ? target.agent : "none"}, /effort only applies to Claude Code`);
      await ev.action.showAlert();
      return;
    }
    try {
      await this.deps.ops.prompt(target.id, `/effort ${effort}`);
      streamDeck.logger.info(`effort: sent /effort ${effort} to ${target.id}`);
    } catch (e) {
      streamDeck.logger.error(`effort ${effort} on ${target.id} failed: ${String(e)}`);
      await ev.action.showAlert();
    }
  }
}

export class PaneDial extends StripDial {
  override readonly manifestId = DIAL_UUIDS.pane;
  override async onDialRotate(ev: DialRotateEvent): Promise<void> {
    const { store } = this.deps;
    if (store.getState().ui.stripMode === "ticker") {
      store.moveTicker(ev.payload.ticks);
      return;
    }
    const target = store.focusedPane();
    if (!target) return;
    try {
      await this.deps.ops.scroll(target.id, scrollDelta(ev.payload.ticks));
    } catch (e) {
      streamDeck.logger.error(`scroll ${target.id} failed: ${String(e)}`);
    }
  }
  override onDialDown(_ev: DialDownEvent): void {
    this.deps.store.toggleStripMode();
  }
}

export function createDials(deps: DialDeps): StripDial[] {
  return [new ScrollDial(deps), new MachineDial(deps), new EffortDial(deps), new PaneDial(deps)];
}
