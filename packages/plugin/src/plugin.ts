// LCARS Stream Deck plugin entry point. Thin by design: subscribe to Ops,
// paint keys and the touch strip, forward presses and dial turns.
// See docs/HANDOFF.md and CLAUDE.md.
import os from "node:os";
import streamDeck from "@elgato/streamdeck";
import { createDials } from "./actions/dials";
import { AttachAction, DictateAction, MuteBellAction, StopAllAction } from "./actions/global-keys";
import { InboxKeyAction } from "./actions/inbox-key";
import { createOsa } from "./os/iterm";
import { createFleetStore } from "./core/store";
import { advance, EMPTY_BASELINE, type Baseline } from "./core/ping-baseline";
import { createBell } from "./os/bell";
import { createOpsActions, type OpsActions } from "./ops/actions";
import { createOpsClient, DEFAULT_OPS_URL, type OpsClient } from "./ops/client";
import { defaultTokenPath, readToken } from "./ops/token";
import { createTerminalActivator } from "./os/terminal";

/** Shared by every LCARS key; set from any Inbox Key property inspector. */
type GlobalSettings = { opsUrl?: string; bell?: "on" | "off"; opsTokenPath?: string };

streamDeck.logger.setLevel("info");

// The deck's own host, so "the focused pane" prefers this machine's Herdr.
const localHost = os.hostname().split(".")[0].toLowerCase();
const store = createFleetStore({ localHost });
let opsUrl = DEFAULT_OPS_URL;
let tokenPath = defaultTokenPath();
const token = (): string | null => readToken(tokenPath);
let client: OpsClient | null = null;
let actions = createOpsActions({ url: opsUrl, token });
// Indirection so a changed Ops address reaches actions created at startup.
const ops: OpsActions = {
  focus: (id) => actions.focus(id),
  scroll: (id, delta) => actions.scroll(id, delta),
  prompt: (id, text) => actions.prompt(id, text),
  sendKeys: (id, keys) => actions.sendKeys(id, keys),
};
let bellEnabled = true;
const bell = createBell({ enabled: () => bellEnabled });

const terminal = createTerminalActivator({ app: process.env.LCARS_TERMINAL_APP });
const inbox = new InboxKeyAction(store, ops, terminal);
const dials = createDials({ store, ops, terminal });
const osa = createOsa();
const stopAll = new StopAllAction(store, ops);
const muteBell = new MuteBellAction({
  isOn: () => bellEnabled,
  set: async (on) => {
    const current = await streamDeck.settings.getGlobalSettings<GlobalSettings>();
    await streamDeck.settings.setGlobalSettings<GlobalSettings>({ ...current, bell: on ? "on" : "off" });
    bellEnabled = on;
  },
});
const globals = [new AttachAction(osa), stopAll, new DictateAction(osa), muteBell];

// Done ping: compare consecutive healthy views. The first view after a
// (re)connect only primes the baseline, so a reconnect never pings for
// agents that were already blocked.
let baseline: Baseline = EMPTY_BASELINE;
store.subscribe((state) => {
  inbox.renderAll();
  for (const dial of dials) dial.renderAll();
  stopAll.renderAll();
  if (!state.healthy) {
    baseline = EMPTY_BASELINE;
    return;
  }
  const { next, flips } = advance(baseline, state.view, store.allKeys());
  baseline = next;
  for (const flip of flips) {
    streamDeck.logger.info(`ping: ${flip.session.id} ${flip.kind}`);
    inbox.flash(flip.session.id);
    bell.play(flip.kind);
  }
});

const applyConnection = (settings: GlobalSettings): void => {
  bellEnabled = settings.bell !== "off";
  muteBell.renderAll();
  const url = settings.opsUrl?.trim() || DEFAULT_OPS_URL;
  const nextTokenPath = settings.opsTokenPath?.trim() || defaultTokenPath();
  if (client && url === opsUrl && nextTokenPath === tokenPath) return;
  opsUrl = url;
  tokenPath = nextTokenPath;
  actions = createOpsActions({ url, token });
  client?.stop();
  client = createOpsClient({
    url,
    token,
    onView: (view) => store.setView(view),
    onDown: (reason) => {
      streamDeck.logger.warn(`ops connection lost: ${reason}`);
      store.setDown();
    },
    onLog: (message) => streamDeck.logger.info(message),
  });
  client.start();
};

streamDeck.settings.onDidReceiveGlobalSettings<GlobalSettings>((ev) => applyConnection(ev.settings));
streamDeck.actions.registerAction(inbox);
for (const dial of dials) streamDeck.actions.registerAction(dial);
for (const key of globals) streamDeck.actions.registerAction(key);

await streamDeck.connect();
streamDeck.logger.info(`lcars plugin up on ${localHost}`);
applyConnection(await streamDeck.settings.getGlobalSettings<GlobalSettings>());
