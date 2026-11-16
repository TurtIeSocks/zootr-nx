import { create } from "zustand";
import { dotPath } from "zustand-dot";
import {
  commands,
  type Label,
  type Status,
  type Summary,
  type View,
} from "@/bindings";
import { call, keep, message } from "@/lib/api";
import { blocked } from "@/lib/config";
import { settingsDiff } from "@/lib/settings";
import { saveConfig, useConfig } from "@/stores/config";
import { once } from "@/stores/once";

/** The targets' status, the seed history, and the actions that write them. */
interface Seeds {
  /** Status and history have been read once. */
  loaded: boolean;
  /** Whether the game dump and each target are there, and which run. */
  status: Status;
  /** The installed seed and every one generated. */
  history: View;
  /** An action is running. */
  busy: boolean;
  /** Why the last action or refresh failed. */
  error?: string;
  /** Why nothing can be installed right now, or "" when something can. */
  blocked: string;
  /** A target's game is running. */
  running: boolean;
  /** The settings that differ from the installed seed's, or from the
   * defaults with none installed, one line each. */
  drift: string[];
  /** How many settings differ from each entry's, by entry id. */
  diffs: Record<number, number>;
}

/** The seeds store. Start it with {@link init}. */
export const useSeeds = create<Seeds>()(
  dotPath(
    (): Seeds => ({
      loaded: false,
      status: { game: false, targets: [] },
      history: { installed: null, entries: [] },
      busy: false,
      blocked: "",
      running: false,
      drift: [],
      diffs: {},
    }),
  ),
);

/** What `drift` is measured from, for its wording. */
export const useDriftFrom = () =>
  useSeeds((s) =>
    s.history.installed ? "the installed seed" : "the default settings",
  );

/** Works out `blocked`, `running`, `drift` and `diffs` from status,
 * history and the config. */
function derive() {
  const { schema, config } = useConfig.getState();
  const { status, history } = useSeeds.getState();
  const diff = (e: Summary) =>
    settingsDiff(schema, e.settings, config.settings);
  useSeeds.setPath("blocked", blocked(status, config));
  useSeeds.setPath(
    "running",
    status.targets.some((t) => t.running),
  );
  useSeeds.setPath(
    "drift",
    keep(
      history.installed
        ? diff(history.installed)
        : settingsDiff(schema, {}, config.settings),
    ),
  );
  useSeeds.setPath(
    "diffs",
    keep(
      Object.fromEntries(history.entries.map((e) => [e.id, diff(e).length])),
    ),
  );
}
useSeeds.subscribePath("status", derive);
useSeeds.subscribePath("history", derive);
useConfig.subscribePath("schema", derive);
useConfig.subscribePath("config", derive);

/** What the last refresh failed with, cleared by the next one that works. */
let failed: string | undefined;

/** Reads status and history again. Unchanged ones keep their identity, so
 * nothing redraws. */
export async function refresh() {
  try {
    const [status, history] = await Promise.all([
      call(commands.status()),
      call(commands.history()),
    ]);
    useSeeds.setPath("status", keep(status));
    useSeeds.setPath("history", keep(history));
    useSeeds.setPath("loaded", true);
    if (failed && useSeeds.getState().error === failed) {
      useSeeds.setPath("error", undefined);
    }
    failed = undefined;
  } catch (e) {
    failed = message(e);
    useSeeds.setPath("error", failed);
  }
}

/** How often status and history are read: a game can start or stop at any
 * time, and a seed made outside the app lands in history. */
const POLL_MS = 2000;

/** Reads status and history now, then every {@link POLL_MS}, on focus,
 * and whenever the config is saved. */
const started = once(async () => {
  await refresh();
  const poll = setInterval(refresh, POLL_MS);
  // A saved config changes what status reports, such as the game dump.
  const unsubscribe = useConfig.subscribePath("config", () => void refresh());
  const focus = () => void refresh();
  window.addEventListener("focus", focus);
  return () => {
    clearInterval(poll);
    unsubscribe();
    window.removeEventListener("focus", focus);
  };
});

export const { init, stop } = started;

/** Runs a write, then reads what it changed. One at a time, sharing one
 * error. */
async function run(action: () => Promise<unknown>) {
  useSeeds.setPath("busy", true);
  useSeeds.setPath("error", undefined);
  try {
    await action();
    await refresh();
  } catch (e) {
    useSeeds.setPath("error", message(e));
  } finally {
    useSeeds.setPath("busy", false);
  }
}

/** Generates and installs a seed. `seed` is typed in, or null for random. */
export const generate = (seed: string | null) =>
  run(() => call(commands.generate(seed)));
/** Installs a history entry to every target. */
export const install = (id: number) => run(() => call(commands.install(id)));
/** Takes the mod off every target. */
export const uninstall = () => run(() => call(commands.uninstall()));
/** Deletes a history entry. */
export const deleteSeed = (id: number) =>
  run(() => call(commands.deleteSeed(id)));
/** Deletes every entry but the installed one. */
export const clearHistory = () => run(() => call(commands.clearHistory()));
/** Makes an entry's settings the current ones. */
export const takeSettings = (entry: Summary) =>
  run(() => saveConfig({ settings: entry.settings }));

/** Names a seed and notes what it's for. Throws what saving failed with. */
export async function label(id: number, label: Label) {
  await call(commands.labelSeed(id, label));
  await refresh();
}
