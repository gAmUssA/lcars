// LCARS Stream Deck plugin entry point. Thin by design: subscribe to Ops,
// paint keys, forward presses. See docs/HANDOFF.md and CLAUDE.md.
import streamDeck from "@elgato/streamdeck";
import { InboxKeyAction } from "./actions/inbox-key";
import { createFleetStore } from "./core/store";
import { createOpsActions } from "./ops/actions";
import { createOpsClient, DEFAULT_OPS_URL, type OpsClient } from "./ops/client";
import { createTerminalActivator } from "./os/terminal";

/** Shared by every LCARS key; set from any Inbox Key property inspector. */
type GlobalSettings = { opsUrl?: string };

streamDeck.logger.setLevel("info");

const store = createFleetStore();
let opsUrl = DEFAULT_OPS_URL;
let client: OpsClient | null = null;
let actions = createOpsActions({ url: opsUrl });

const terminal = createTerminalActivator({ app: process.env.LCARS_TERMINAL_APP });
const inbox = new InboxKeyAction(store, { focus: (id) => actions.focus(id) }, terminal);

store.subscribe(() => inbox.renderAll());

const applyConnection = (settings: GlobalSettings): void => {
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

await streamDeck.connect();
applyConnection(await streamDeck.settings.getGlobalSettings<GlobalSettings>());
