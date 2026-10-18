// LCARS Stream Deck plugin entry point. Thin by design: subscribe to Ops,
// paint keys and the touch strip, forward presses and dial turns.
// See docs/HANDOFF.md and CLAUDE.md.
import os from "node:os";
import streamDeck from "@elgato/streamdeck";
import { createDials } from "./actions/dials";
import { InboxKeyAction } from "./actions/inbox-key";
import { createFleetStore } from "./core/store";
import { detectFlips } from "./core/transitions";
import { createBell } from "./os/bell";
import type { Session } from "./ops/types";
import { createOpsActions, type OpsActions } from "./ops/actions";
import { createOpsClient, DEFAULT_OPS_URL, type OpsClient } from "./ops/client";
import { createTerminalActivator } from "./os/terminal";

/** Shared by every LCARS key; set from any Inbox Key property inspector. */
type GlobalSettings = { opsUrl?: string; bell?: "on" | "off" };

streamDeck.logger.setLevel("info");

// The deck's own host, so "the focused pane" prefers this machine's Herdr.
const localHost = os.hostname().split(".")[0].toLowerCase();
const store = createFleetStore({ localHost });
let opsUrl = DEFAULT_OPS_URL;
let client: OpsClient | null = null;
let actions = createOpsActions({ url: opsUrl });
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

// Done ping: compare consecutive healthy views. The first view after a
// (re)connect only primes the baseline, so a reconnect never pings for
// agents that were already blocked.
let previous: Session[] | null = null;
store.subscribe((state) => {
  inbox.renderAll();
  for (const dial of dials) dial.renderAll();
  if (!state.healthy) {
    previous = null;
    return;
  }
  const current = store.allKeys();
  if (previous !== null) {
    for (const flip of detectFlips(previous, current)) {
      streamDeck.logger.info(`ping: ${flip.session.id} ${flip.kind}`);
      inbox.flash(flip.session.id);
      bell.play(flip.kind);
    }
  }
  previous = current;
});

const applyConnection = (settings: GlobalSettings): void => {
  bellEnabled = settings.bell !== "off";
  const url = settings.opsUrl?.trim() || DEFAULT_OPS_URL;
  if (client && url === opsUrl) return;
  opsUrl = url;
  actions = createOpsActions({ url });
  client?.stop();
  client = createOpsClient({
    url,
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

await streamDeck.connect();
streamDeck.logger.info(`lcars plugin up on ${localHost}`);
applyConnection(await streamDeck.settings.getGlobalSettings<GlobalSettings>());
