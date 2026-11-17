import { Channel } from "@tauri-apps/api/core";
import { create } from "zustand";
import { dotPath } from "zustand-dot";
import {
  type Build,
  commands,
  type End,
  type Label,
  type Status,
  type Step,
  type StepEvent,
  type Summary,
  type View,
} from "@/bindings";
import { call, keep, message } from "@/lib/api";
import { blocked } from "@/lib/config";
import { settingsDiff } from "@/lib/settings";
import { saveConfig, useConfig } from "@/stores/config";
import { once } from "@/stores/once";

/** A step of a generate run, as the New Seed card lists it. */
export interface StepRow {
  step: Step;
  /** What it's doing now, or did last. */
  detail: string;
  /** Its time once it ended, null while it runs. */
  secs: number | null;
  /** When it started, by `Date.now()`, for its live timer. */
  started: number;
}

/** The running or last generate run. */
export interface Steps {
  rows: StepRow[];
  /** How it ended, once it has. */
  end: End | null;
  /** Its whole time, once it ended. */
  secs: number;
  /** What a stop left, in the stop's words. */
  stopped?: string;
}

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
  /** The running or last generate run's steps, or null before the first. */
  steps: Steps | null;
  /** A seed is being generated, so it can be stopped. */
  generating: boolean;
  /** Stop was pressed and the run hasn't ended yet. */
  stopping: boolean;
  /** The target `steps` rebuilt, or null when they generated a seed. */
  rebuilt: string | null;
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
      steps: null,
      generating: false,
      stopping: false,
      rebuilt: null,
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

/** `steps` with `event` applied. Detail and an end land on the last row,
 * the step running. */
export function stepped(steps: Steps, event: StepEvent): Steps {
  const last = (row: Partial<StepRow>) =>
    steps.rows.map((r, i) =>
      i === steps.rows.length - 1 ? { ...r, ...row } : r,
    );
  switch (event.kind) {
    case "began": {
      const row = {
        step: event.step,
        detail: "",
        secs: null,
        started: Date.now(),
      };
      return { ...steps, rows: [...steps.rows, row] };
    }
    case "detail":
      return { ...steps, rows: last({ detail: event.text }) };
    case "ended":
      return {
        ...steps,
        rows: last({ detail: event.detail, secs: event.secs ?? 0 }),
      };
    case "end":
      return { ...steps, end: event.end, secs: event.secs ?? 0 };
  }
}

/** Sends a generate run's command a channel its steps come back on. A stop
 * isn't an error: its words go with the steps. The command's answer can
 * beat the channel's last event, so a stop is also told by its words,
 * which `Stopped` in zootr's progress module always starts with. Says
 * whether the run finished rather than stopped. */
async function stepping(
  send: (steps: Channel<StepEvent>) => Promise<unknown>,
  rebuilt: string | null = null,
): Promise<boolean> {
  const channel = new Channel<StepEvent>((event) =>
    useSeeds.setPath("steps", (s) => s && stepped(s, event)),
  );
  useSeeds.setPath("steps", { rows: [], end: null, secs: 0 });
  useSeeds.setPath("rebuilt", rebuilt);
  useSeeds.setPath("generating", true);
  try {
    await send(channel);
    return true;
  } catch (e) {
    const text = message(e);
    const stopped =
      useSeeds.getState().steps?.end?.kind === "stopped" ||
      text.startsWith("Stopped");
    if (!stopped) throw e;
    useSeeds.setPath("steps.stopped", text);
    return false;
  } finally {
    useSeeds.setPath("generating", false);
    useSeeds.setPath("stopping", false);
  }
}

/** Generates and installs a seed. `seed` is typed in, or null for random. */
export const generate = (seed: string | null) =>
  run(() => stepping((steps) => call(commands.generate(seed, steps))));
/** Installs a history entry to every target. */
export const install = (id: number) =>
  run(() => stepping((steps) => call(commands.install(id, steps))));
/** Rebuilds a target's game, its output in the step log, and says whether
 * it finished rather than stopped. Throws why it failed, or why the build
 * still lacks the installed hook patch. */
export async function rebuild(name: string) {
  useSeeds.setPath("busy", true);
  try {
    return await stepping((steps) => call(commands.rebuild(name, steps)), name);
  } finally {
    useSeeds.setPath("busy", false);
    await refresh();
  }
}
/** Asks the seed being generated, or the game being rebuilt, to stop. */
export async function stopGenerate() {
  useSeeds.setPath("stopping", true);
  await commands.stopGenerate();
}
/** The last generate run's log. Throws why it couldn't be read. */
export const generateLog = () => call(commands.generateLog());
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

/** A target's build status, for a game that bakes the hook patch in when
 * it's built. */
export const useBuild = (name: string | undefined) =>
  useSeeds((s) => s.status.targets.find((t) => t.name === name)?.build);

/** Whether the game would run another hook than the installed one. */
export const stale = (build: Build | null | undefined) =>
  build?.status.kind === "stale" || build?.status.kind === "refused";

/** Names a seed and notes what it's for. Throws what saving failed with. */
export async function label(id: number, label: Label) {
  await call(commands.labelSeed(id, label));
  await refresh();
}
