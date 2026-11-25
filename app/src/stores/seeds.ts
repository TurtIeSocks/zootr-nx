import { Channel } from "@tauri-apps/api/core";
import { create } from "zustand";
import { dotPath } from "zustand-dot";
import {
  type Build,
  commands,
  type End,
  type Label,
  STEPS,
  type Status,
  type Step,
  type StepEvent,
  type Summary,
  type View,
} from "@/bindings";
import { call, keep, message } from "@/lib/api";
import { blocked, noDump, type Settings, unbuildable } from "@/lib/config";
import { NO_PANES, type PaneChange, paneChanges, resetPane } from "@/lib/drift";
import { count } from "@/lib/format";
import { type Change, settingsChanges } from "@/lib/settings";
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

/** A step's name, capitalized. */
export function stepName(step: Step) {
  const name = STEPS.find((s) => s.step === step)?.name ?? step;
  return name.charAt(0).toUpperCase() + name.slice(1);
}

/** The running or last generate run. */
export interface Steps {
  rows: StepRow[];
  /** How it ended, once it has. */
  end: End | null;
  /** Its whole time, once it ended. */
  secs: number;
  /** How many steps a run of its kind takes, for its progress. */
  total?: number;
  /** It installs an Archipelago slot, so its steps show in that folder. */
  ap?: boolean;
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
  /** Why no seed can be built right now, even for History alone. */
  unbuildable: string;
  /** A target's game is running. */
  running: boolean;
  /** The settings that differ from the installed seed's, or from the
   * defaults with none installed. */
  drift: Change[];
  /** The palettes, pools, tracks and pins that differ from the same seed's,
   * or from none set with none installed. */
  paneDrift: PaneChange[];
  /** How many settings and pane entries differ from each entry's, by entry
   * id. */
  diffs: Record<number, number>;
  /** The running or last generate run's steps, or null before the first. */
  steps: Steps | null;
  /** A seed is being generated, so it can be stopped. */
  generating: boolean;
  /** Stop was pressed and the run hasn't ended yet. */
  stopping: boolean;
  /** The target `steps` rebuilt, or null when they generated a seed. */
  rebuilt: string | null;
  /** The History entry being installed, or null while none is. */
  installing: number | null;
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
      unbuildable: "",
      running: false,
      drift: [],
      paneDrift: [],
      diffs: {},
      steps: null,
      generating: false,
      stopping: false,
      rebuilt: null,
      installing: null,
    }),
  ),
);

/** The seed `drift` is measured from: the installed one, unless it's an
 * Archipelago slot, whose settings came with its file. Then the newest
 * standard seed, or null for the defaults. */
function driftBase({ installed, entries }: View): Summary | null {
  if (!installed?.ap_slot) return installed;
  return entries.find((e) => !e.ap_slot) ?? null;
}

/** The seed `drift` is measured from, or null for the defaults. */
export const useDriftBase = () => useSeeds((s) => driftBase(s.history));

/** Whether the target named `name` is running. */
export const useTargetRunning = (name: string | undefined) =>
  useSeeds((s) => s.status.targets.some((t) => t.name === name && t.running));

/** No settings: the defaults, as a base that keeps its identity. */
const NONE: Settings = {};

/** The settings `drift` is measured from, `{}` for the defaults. */
export const useDriftSettings = () =>
  useSeeds((s) => driftBase(s.history)?.settings ?? NONE);

/** Whether setting `id` is in `drift`: what a change dot marks. */
export const useDrifted = (id: string) =>
  useSeeds((s) => s.drift.some((c) => c.def.id === id));

/** Whether group `name` holds a setting in `drift`. */
export const useGroupDrifted = (name: string) =>
  useSeeds((s) => s.drift.some((c) => c.def.group === name));

/** What `drift` is measured from, as `useDriftFrom` words it. */
export const DEFAULTS_FROM = "the default settings";

/** "since" a seed, "from" the defaults. */
export const driftSince = (from: string) =>
  from === DEFAULTS_FROM ? "from" : "since";

/** How many changes, and from what: "3 changes since the last seed". */
export const driftWords = (n: number, from: string) =>
  `${count(n, "change")} ${driftSince(from)} ${from}`;

/** How many settings and pane entries drift holds. */
export const useDriftCount = () =>
  useSeeds((s) => s.drift.length + s.paneDrift.length);

/** The drift count in words, as the drift bar and Play's pill say it. */
export const useDriftWords = () => driftWords(useDriftCount(), useDriftFrom());

/** What `drift` is measured from, for its wording. */
export const useDriftFrom = () =>
  useSeeds((s) => (driftBase(s.history) ? "the last seed" : DEFAULTS_FROM));

/** Works out `blocked`, `running`, `drift` and `diffs` from status,
 * history and the config. */
function derive() {
  const { schema, config } = useConfig.getState();
  const { status, history } = useSeeds.getState();
  const diff = (e: Summary) =>
    settingsChanges(schema, e.settings, config.settings);
  const panes = (e: Summary) => paneChanges(e, config);
  useSeeds.setPath("blocked", blocked(status, config));
  useSeeds.setPath("unbuildable", unbuildable(status, config));
  useSeeds.setPath(
    "running",
    status.targets.some((t) => t.running),
  );
  const base = driftBase(history);
  useSeeds.setPath(
    "drift",
    keep(base ? diff(base) : settingsChanges(schema, {}, config.settings)),
  );
  useSeeds.setPath("paneDrift", keep(paneChanges(base ?? NO_PANES, config)));
  useSeeds.setPath(
    "diffs",
    keep(
      Object.fromEntries(
        history.entries.map((e) => [e.id, diff(e).length + panes(e).length]),
      ),
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

/** What a run is, for its steps. */
interface Run {
  /** How many steps its kind takes. */
  total: number;
  /** The target it rebuilds, for a rebuild. */
  rebuilt?: string | null;
  /** It installs an Archipelago slot. */
  ap?: boolean;
}

/** Sends a generate run's command a channel its steps come back on. A stop
 * isn't an error: its words go with the steps. The command's answer can
 * beat the channel's last event, so a stop is also told by its words,
 * which `Stopped` in zootr's progress module always starts with. Says
 * whether the run finished rather than stopped. */
async function stepping(
  send: (steps: Channel<StepEvent>) => Promise<unknown>,
  { total, rebuilt = null, ap }: Run,
): Promise<boolean> {
  const channel = new Channel<StepEvent>((event) =>
    useSeeds.setPath("steps", (s) => s && stepped(s, event)),
  );
  useSeeds.setPath("steps", { rows: [], end: null, secs: 0, total, ap });
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

/** The steps of a run that builds a seed and installs it: every step up to
 * installing. */
const installed = () => STEPS.findIndex((s) => s.step === "installing") + 1;

/** Generates a seed and installs it, or with `install` false only adds it
 * to History. `seed` is typed in, or null for random. */
export const generate = (seed: string | null, install = true) =>
  run(() =>
    stepping((steps) => call(commands.generate(seed, install, steps)), {
      total: install ? installed() : installed() - 1,
    }),
  );
/** Installs one slot of an Archipelago multiworld from its `.apznx`. */
export const generateAp = (path: string) =>
  run(() =>
    stepping((steps) => call(commands.generateAp(path, steps)), {
      total: installed(),
      ap: true,
    }),
  );
/** Installs a history entry to every target. */
export async function install(id: number) {
  useSeeds.setPath("installing", id);
  try {
    await run(() =>
      stepping((steps) => call(commands.install(id, steps)), { total: 1 }),
    );
  } finally {
    useSeeds.setPath("installing", null);
  }
}
/** Installs the installed seed again in the Colors and Audio drafts: the
 * same shuffle, painted and scored again. */
export const applyCosmetics = () =>
  run(() =>
    stepping((steps) => call(commands.applyCosmetics(steps)), { total: 1 }),
  );
/** Rebuilds a target's game, its output in the step log, and says whether
 * it finished rather than stopped. Throws why it failed, or why the build
 * still lacks the installed hook patch. */
export async function rebuild(name: string) {
  useSeeds.setPath("busy", true);
  try {
    return await stepping((steps) => call(commands.rebuild(name, steps)), {
      total: 1,
      rebuilt: name,
    });
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
/** Makes an entry's settings the current ones. `{ settings: {} }` puts
 * them back to the defaults. */
export const takeSettings = (entry: Pick<Summary, "settings">) =>
  run(() => saveConfig({ settings: entry.settings }));

/** Makes what drift measures from the current settings and panes: `base`'s,
 * or the defaults with none. Locked palettes keep their picks. */
export const takeSeed = (base: Summary | null) =>
  run(() =>
    saveConfig((c) => ({
      settings: base?.settings ?? {},
      ...Object.assign(
        {},
        ...(["colors", "audio", "plando"] as const).map((pane) =>
          resetPane(pane, base ?? NO_PANES)(c),
        ),
      ),
    })),
  );

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

/** Why there's no game dump to read, or "" when there is one. */
export function useNoDump(): string {
  const status = useSeeds((s) => s.status);
  const config = useConfig((s) => s.config);
  return noDump(status, config);
}
