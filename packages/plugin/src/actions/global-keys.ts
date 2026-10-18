// Global keys: Attach (open a terminal on a host's Herdr), Stop All (two-press
// interrupt of every busy agent), Dictate (macOS dictation into iTerm2) and
// Mute Bell. They share the simple label-key renderer.
import streamDeck, {
  SingletonAction,
  type DidReceiveSettingsEvent,
  type KeyDownEvent,
  type WillAppearEvent,
} from "@elgato/streamdeck";
import { createArmedTrigger, type ArmedTrigger } from "../core/arm";
import { renderLabelKey } from "../core/render";
import type { FleetStore } from "../core/store";
import { attachCommand, openWindowScript, startDictationScript, type AttachSettings, type Osa } from "../os/iterm";
import type { OpsActions } from "../ops/actions";
import type { Session } from "../ops/types";

export const GLOBAL_UUIDS = {
  attach: "io.gamov.lcars.attach",
  stopAll: "io.gamov.lcars.stop-all",
  dictate: "io.gamov.lcars.dictate",
  muteBell: "io.gamov.lcars.mute-bell",
} as const;

/** Agents Stop All interrupts: anything mid-turn or waiting on a dialog. */
export function busySessions(keys: Session[]): Session[] {
  return keys.filter((s) => s.state === "working" || s.state === "blocked");
}

/** What Stop All sends: Escape interrupts Claude Code and Codex turns and dismisses dialogs. */
export const STOP_KEYS = ["esc"];

export class AttachAction extends SingletonAction<AttachSettings> {
  override readonly manifestId = GLOBAL_UUIDS.attach;
  constructor(private readonly osa: Osa) {
    super();
  }
  override onWillAppear(ev: WillAppearEvent<AttachSettings>): void {
    if (ev.action.isKey()) this.paint(ev.action, ev.payload.settings);
  }
  override onDidReceiveSettings(ev: DidReceiveSettingsEvent<AttachSettings>): void {
    if (ev.action.isKey()) this.paint(ev.action, ev.payload.settings);
  }
  override async onKeyDown(ev: KeyDownEvent<AttachSettings>): Promise<void> {
    try {
      const command = attachCommand(ev.payload.settings);
      await this.osa.run(openWindowScript(command));
      streamDeck.logger.info(`attach: opened terminal running ${command}`);
    } catch (e) {
      streamDeck.logger.error(`attach failed: ${String(e)}`);
      await ev.action.showAlert();
    }
  }
  private paint(action: { setImage(image: string): Promise<void>; setTitle(title: string): Promise<void> }, settings: AttachSettings): void {
    const target = (settings.target ?? "").trim() || "local";
    const mode = settings.mode ?? "herdr";
    void action.setTitle("");
    void action.setImage(renderLabelKey({ title: target.toUpperCase().slice(0, 8), subtitle: mode === "tmux" ? "tmux" : mode === "mosh" ? "mosh herdr" : "herdr", color: "#5AA0E0" }));
  }
}

export class StopAllAction extends SingletonAction {
  override readonly manifestId = GLOBAL_UUIDS.stopAll;
  readonly trigger: ArmedTrigger;
  #timer: ReturnType<typeof setTimeout> | null = null;
  constructor(
    private readonly store: FleetStore,
    private readonly ops: OpsActions,
    now: () => number = Date.now,
  ) {
    super();
    this.trigger = createArmedTrigger({ now });
  }
  override onWillAppear(): void {
    this.renderAll();
  }
  override async onKeyDown(ev: KeyDownEvent): Promise<void> {
    const targets = busySessions(this.store.allKeys());
    if (this.trigger.press() === "armed") {
      this.renderAll();
      if (this.#timer) clearTimeout(this.#timer);
      this.#timer = setTimeout(() => {
        this.trigger.disarm();
        this.renderAll();
      }, this.trigger.remainingMs() + 50);
      return;
    }
    if (this.#timer) clearTimeout(this.#timer);
    let failed = 0;
    for (const s of targets) {
      try {
        await this.ops.sendKeys(s.id, STOP_KEYS);
      } catch (e) {
        failed += 1;
        streamDeck.logger.error(`stop-all: ${s.id} failed: ${String(e)}`);
      }
    }
    streamDeck.logger.info(`stop-all: interrupted ${targets.length - failed} of ${targets.length} agents`);
    if (failed > 0) await ev.action.showAlert();
    this.renderAll();
  }
  renderAll(): void {
    const count = busySessions(this.store.allKeys()).length;
    const armed = this.trigger.isArmed();
    const image = renderLabelKey({
      title: armed ? "SURE?" : "STOP",
      subtitle: armed ? `press again · ${count}` : `${count} busy`,
      color: armed ? "#D13438" : "#7f1d1d",
    });
    this.actions.forEach((a) => {
      if (!a.isKey()) return;
      void a.setTitle("");
      void a.setImage(image);
    });
  }
}

export class DictateAction extends SingletonAction {
  override readonly manifestId = GLOBAL_UUIDS.dictate;
  constructor(private readonly osa: Osa) {
    super();
  }
  override onWillAppear(ev: WillAppearEvent): void {
    if (!ev.action.isKey()) return;
    void ev.action.setTitle("");
    void ev.action.setImage(renderLabelKey({ title: "DICTATE", subtitle: "into iTerm2", color: "#9A7BD0" }));
  }
  override async onKeyDown(ev: KeyDownEvent): Promise<void> {
    try {
      await this.osa.run(startDictationScript());
      streamDeck.logger.info("dictate: started macOS dictation in iTerm2");
    } catch (e) {
      streamDeck.logger.error(`dictate failed: ${String(e)}`);
      await ev.action.showAlert();
    }
  }
}

export type BellControl = { isOn(): boolean; set(on: boolean): Promise<void> };

export class MuteBellAction extends SingletonAction {
  override readonly manifestId = GLOBAL_UUIDS.muteBell;
  constructor(private readonly bell: BellControl) {
    super();
  }
  override onWillAppear(): void {
    this.renderAll();
  }
  override async onKeyDown(): Promise<void> {
    await this.bell.set(!this.bell.isOn());
    this.renderAll();
  }
  renderAll(): void {
    const on = this.bell.isOn();
    const image = renderLabelKey({ title: on ? "BELL" : "MUTED", subtitle: on ? "press to mute" : "press to unmute", color: on ? "#22C55E" : "#374151" });
    this.actions.forEach((a) => {
      if (!a.isKey()) return;
      void a.setTitle("");
      void a.setImage(image);
    });
  }
}
