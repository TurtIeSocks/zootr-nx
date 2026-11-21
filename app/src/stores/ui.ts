import { getCurrentWindow } from "@tauri-apps/api/window";
import { create } from "zustand";
import {
  type PersistStorage,
  persist,
  type StorageValue,
} from "zustand/middleware";
import { dotPath } from "zustand-dot";
import {
  commands,
  type Edit,
  events,
  type FlagQuery,
  type Side,
  type Value,
} from "@/bindings";
import type { Filter } from "@/checks/rows";
import { call, message } from "@/lib/api";
import { THEMES, type Theme } from "@/lib/theme";
import type { SpoilerTab } from "@/play/spoiler/rows";
import type { SaveTab } from "@/save/tabs";
import type { Section, Tab } from "@/shell/nav";

/** Live's resizable columns, in pixels. Change takes whatever is left. */
export const COLUMNS = { time: 48, name: 360, type: 112 };
/** Live's column widths, by column. */
export type Widths = typeof COLUMNS;

/** View state, kept in `ui.json` across tabs and restarts. */
/** The Play page's folders. */
export type PlayTab = "standard" | "archipelago" | "history";

/** Which seeds History lists. */
export type HistoryFilter = "all" | "standard" | "archipelago";

/** The dev panel's tabs. */
export type DevTab = "warp" | "actors" | "give" | "values" | "tests";

/** A give the dev panel repeats: the item and how many. */
export interface DevGive {
  item: string;
  count: number;
}

export interface Ui {
  /** The view showing. */
  tab: Tab | "Settings";
  /** The Settings page showing. */
  section: Section;
  /** Shared: every window shows the main window's. */
  theme: Theme;
  /** The Live tab's columns, view and filter. */
  live: {
    columns: Widths;
    /** Quiet hides unnamed values. */
    quiet: boolean;
    /** The filter typed over the log. */
    grep: string;
    /** Back to back changes of one value show as one row. */
    fold: boolean;
    /** Kinds of value the log leaves out. A kind not listed shows. */
    hiddenKinds: string[];
    /** Where you had reached: the session's log and the next row's id.
     * Rows after it flash. */
    seen: { log: string | null; id: number };
  };
  /** Checks' search and filter. */
  checks: { query: string; filter: Filter };
  /** The spoiler's tab, its Way of the Hero filter, and its checks
   * search. */
  spoiler: { tab: SpoilerTab; heroOnly: boolean; query: string };
  /** The seed or share code typed into New seed. */
  newSeed: { seed: string };
  /** The Play page's open folder, or null to pick by the installed seed,
   * and History's filter. */
  play: { tab: PlayTab | null; history: HistoryFilter };
  /** Each Archipelago seed's server and password, by history id. */
  ap: { rooms: Record<number, { server: string; password: string }> };
  /** The Items tab's banner about zootr-nx's own icons was dismissed. */
  items: { bannerDismissed: boolean };
  /** The first-launch welcome screen was finished or skipped. */
  welcome: { done: boolean };
  /** Play's Get started card was dismissed. */
  getStarted: { dismissed: boolean };
  /** The release the update notice was told to skip. */
  update: { skipped: string | null };
  /** The sections closed on the settings pages, by `group/section`, and
   * the reset a page's split button leads with: the one used last. */
  settings: { closed: string[]; reset: ResetTo };
  /** The Save page: the slot, its tab, what's pending and the Flags
   * tab's query. */
  save: {
    slot: number | null;
    /** When the slot's file was written, as last read. Saving over a file
     * written since is refused. */
    stamp: number | null;
    tab: SaveTab;
    presets: string[];
    edits: Edit[];
    /** How many things `edits` change, as the last stage counted. */
    changes: number;
    flags: FlagQuery;
  };
  /** The dev panel's picks and drafts. */
  dev: {
    /** The open tab. */
    tab: DevTab;
    /** Warp's tab, each tab's search, and the side to warp to. */
    teleport: {
      tab: "places" | "actors";
      places: string;
      actors: string;
      side: Side;
    };
    /** Give's item and how many. */
    give: { item: string; count: number };
    /** The GameData name picked in Set. */
    set: { name: string };
    /** The quest and step picked in Quest step. */
    quest: { challenge: string; step: string };
    /** Actors' tab. */
    actors: { tab: "nearby" | "all" };
    /** The places starred, `MAP/SPOT`, in the order starred. */
    starred: string[];
    /** What was done lately, newest first, for one-click repeats. */
    recent: {
      /** Places warped to. */
      places: string[];
      gives: DevGive[];
      /** GameData names set, with the value set. */
      values: { name: string; value: string }[];
    };
  };
}

const INITIAL: Ui = {
  tab: "Play",
  section: "settings",
  theme: "system",
  live: {
    columns: COLUMNS,
    quiet: true,
    grep: "",
    fold: true,
    hiddenKinds: [],
    seen: { log: null, id: 0 },
  },
  checks: { query: "", filter: "all" },
  spoiler: { tab: "spheres", heroOnly: false, query: "" },
  newSeed: { seed: "" },
  play: { tab: null, history: "all" },
  ap: { rooms: {} },
  items: { bannerDismissed: false },
  welcome: { done: false },
  getStarted: { dismissed: false },
  update: { skipped: null },
  settings: { closed: [], reset: "defaults" },
  save: {
    slot: null,
    stamp: null,
    tab: "items",
    presets: [],
    edits: [],
    changes: 0,
    flags: { search: "", kind: null, changed: true, staged: false, open: {} },
  },
  dev: {
    tab: "warp",
    teleport: { tab: "places", places: "", actors: "", side: "front" },
    give: { item: "", count: 1 },
    set: { name: "" },
    quest: { challenge: "", step: "" },
    actors: { tab: "nearby" },
    starred: [],
    recent: { places: [], gives: [], values: [] },
  },
};

/** Keys the main window sets for every window. */
const SHARED = ["theme"] as const satisfies (keyof Ui)[];

/** Each step upgrades a saved shape one version. A changed shape adds one. */
type Upgrade = (saved: Record<string, unknown>) => Record<string, unknown>;
const UPGRADES: Upgrade[] = [
  // Settings checklists started closed, kept by `open`. Every section now
  // starts open, kept by `closed`.
  (saved) => ({ ...saved, settings: { closed: [] } }),
];

type Saved = StorageValue<Ui>;
/** At most one write this often, so typing saves once a beat. */
const WAIT = 300;

const isObject = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

/** `saved` over `defaults`, branch by branch, so a branch saved before a
 * field was added still gets that field's default. */
export function fill<T>(defaults: T, saved: unknown): T {
  if (!isObject(defaults) || !isObject(saved)) return defaults;
  const out: Record<string, unknown> = { ...defaults };
  for (const [key, value] of Object.entries(saved)) {
    const had = out[key];
    out[key] = isObject(had) ? fill(had, value) : value;
  }
  return out as T;
}

/** The two localStorage keys from before `ui.json`. */
const LEGACY = { theme: "theme", columns: "live-columns" };

function legacy(): Record<string, unknown> {
  try {
    const theme = THEMES.find((t) => t === localStorage.getItem(LEGACY.theme));
    const columns = JSON.parse(localStorage.getItem(LEGACY.columns) ?? "null");
    return { ...(theme && { theme }), ...(columns && { live: { columns } }) };
  } catch {
    return {};
  }
}

/** Removes the old keys. True when there were any. */
function dropLegacy(): boolean {
  try {
    const had = Object.values(LEGACY).some((k) => localStorage.getItem(k));
    for (const k of Object.values(LEGACY)) localStorage.removeItem(k);
    return had;
  } catch {
    return false;
  }
}

/** `ui.json` through the backend: this window's part, plus the shared part. */
export const backend: PersistStorage<Ui> = {
  getItem: async () => {
    const { own, shared } = await call(commands.uiLoad());
    if (!own && !shared) return null;
    const saved = (own ?? { state: {}, version: UPGRADES.length }) as Saved;
    return { ...saved, state: { ...saved.state, ...(shared as object) } };
  },
  setItem: async (_, { state, version }) => {
    const own: Record<string, unknown> = { ...state };
    const shared: Record<string, unknown> = {};
    for (const key of SHARED) {
      shared[key] = state[key];
      delete own[key];
    }
    await call(
      commands.uiSave({ state: own, version } as Value, shared as Value),
    );
  },
  removeItem: () => {},
};

/**
 * `storage` with at most one write every `WAIT` ms. A change after a quiet
 * spell is written at once, and those within the next `WAIT` ms go as one
 * write at its end, the last of them. So a steady stream (Live marking rows
 * seen) still saves every beat, and a quit, which skips `flush`, loses at
 * most one beat's changes.
 */
export function batched(storage: PersistStorage<Ui>) {
  let pending: [string, Saved] | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const write = (item: [string, Saved]) =>
    Promise.resolve(storage.setItem(...item)).catch((e) =>
      console.error(`Couldn't save the view: ${message(e)}`),
    );
  const flush = async () => {
    clearTimeout(timer);
    timer = undefined;
    const item = pending;
    pending = undefined;
    if (item) await write(item);
  };
  const setItem = (name: string, value: Saved) => {
    if (timer) {
      pending = [name, value];
      return;
    }
    timer = setTimeout(flush, WAIT);
    void write([name, value]);
  };
  return { ...storage, setItem, flush };
}

/** A storage held in memory, for tests. */
export function memory(): PersistStorage<Ui> & { saved: Map<string, Saved> } {
  const saved = new Map<string, Saved>();
  return {
    saved,
    getItem: (name) => saved.get(name) ?? null,
    setItem: (name, value) => void saved.set(name, value),
    removeItem: (name) => void saved.delete(name),
  };
}

export function createUi(
  storage: PersistStorage<Ui>,
  upgrades: Upgrade[] = UPGRADES,
) {
  return create<Ui>()(
    persist(
      dotPath(() => INITIAL),
      {
        name: "ui",
        storage,
        version: upgrades.length,
        // `merge` fills in whatever an upgraded shape still lacks.
        migrate: (saved, version) =>
          upgrades
            .slice(version)
            .reduce(
              (s, up) => up(s),
              saved as Record<string, unknown>,
            ) as unknown as Ui,
        // Nothing saved yet: take what localStorage kept before ui.json.
        merge: (saved, current) => fill(current, saved ?? legacy()),
        skipHydration: true,
      },
    ),
  );
}

const file = batched(backend);
// Tests never reach the backend.
export const useUi = createUi(
  import.meta.env.MODE === "test" ? memory() : file,
);

/** Takes the main window's shared part, unless it already has it. */
function takeShared(shared: Value) {
  const state = useUi.getState();
  const next = fill(state, shared);
  if (SHARED.some((k) => next[k] !== state[k])) useUi.setState(next, true);
}

let loading: Promise<void> | undefined;

/** Loads this window's view state, once. Each window's root waits on it. */
export function init(): Promise<void> {
  loading ??= (async () => {
    await useUi.persist.rehydrate();
    // Hydrating doesn't save, so what came from localStorage is saved now.
    if (dropLegacy()) useUi.setState(useUi.getState());
    await events.uiShared.listen((e) => takeShared(e.payload));
    // The last change before closing would otherwise wait out `WAIT`.
    await getCurrentWindow().onCloseRequested(file.flush);
  })();
  return loading;
}

/** `list` with `item` first and any equal one dropped, at most `max`
 * long: a recents list. */
export function remember<T>(
  list: T[],
  item: T,
  max: number,
  same: (a: T, b: T) => boolean = (a, b) => a === b,
): T[] {
  return [item, ...list.filter((x) => !same(x, item))].slice(0, max);
}

/** Opens or closes a settings page's section, by its id. */
/** What a settings page's reset puts back: the defaults, or the seed
 * drift measures from. */
export type ResetTo = "defaults" | "seed";

export const setSectionOpen = (id: string, open: boolean) =>
  useUi.setPath("settings.closed", (c) =>
    open ? c.filter((t) => t !== id) : [...c, id],
  );
