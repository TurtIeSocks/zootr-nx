import { create } from "zustand";
import { dotPath } from "zustand-dot";
import {
  type Cell,
  commands,
  events,
  type ItemsState,
  type Placement,
  type Section,
  type TrackerOptions,
} from "@/bindings";
import { TITLES } from "@/items/layout";
import { call, message } from "@/lib/api";
import { once } from "@/stores/once";

/** Where the tiles' icons come from: zootr-nx's own, the dump's while
 * they build, or the dump's. */
export type Art = "fallback" | "building" | "game";

/** The tracker's options, every one filled in. */
export type Options = Required<TrackerOptions>;

/** `o` with defaults for what it leaves out. */
export const fullOptions = (o: TrackerOptions): Options => ({
  size: "Medium",
  background: "#18181b",
  sections: [],
  ...o,
});

/** The grid and its options, kept current by the backend's events. Every
 * window gets the same events, so the tab and the pop out agree. */
interface Items {
  /** The grid has come from the backend. */
  loaded: boolean;
  /** Every tile, in section and grid order. */
  cells: Cell[];
  /** Size, background and the sections' layout. */
  options: Options;
  /** Hidden sections, in a fixed order so a chip never moves while you
   * reach for it. */
  hidden: Section[];
  /** Where the tiles' icons come from. */
  art: Art;
  /** Why loading, building the icons or saving the options last failed. */
  error?: string;
}

/** The items store. Start it with {@link init}. */
export const useItems = create<Items>()(
  dotPath(
    (): Items => ({
      loaded: false,
      cells: [],
      options: fullOptions({}),
      hidden: [],
      art: "fallback",
    }),
  ),
);

useItems.subscribePath("options.sections", (sections) =>
  useItems.setPath(
    "hidden",
    (Object.keys(TITLES) as Section[]).filter((s) =>
      sections.some((p) => p.section === s && !p.shown),
    ),
  ),
);

/** Keeps why something failed, for the tab to show. */
const fail = (e: unknown) => useItems.setPath("error", message(e));

/** The last state's `game_art`: the dump's icons are wanted. */
let wanted = false;

/** Follows `game_art`: zootr-nx's icons while it's off; once it turns on,
 * builds the dump's, falling back to zootr-nx's if that fails. */
function showArt(gameArt: boolean) {
  const was = wanted;
  wanted = gameArt;
  if (!gameArt) return useItems.setPath("art", "fallback");
  // Already built, building, or failed since it turned on.
  if (was) return;
  useItems.setPath("art", "building");
  call(commands.itemsIcons())
    .then(() => wanted && useItems.setPath("art", "game"))
    .catch((e) => {
      fail(e);
      useItems.setPath("art", "fallback");
    });
}

/** Takes the backend's whole state. */
function put({ cells, options, game_art }: ItemsState) {
  useItems.setState((s) => ({
    ...s,
    loaded: true,
    cells,
    options: fullOptions(options),
  }));
  showArt(game_art);
}

/** Listens for the backend's state and loads it. Each state says whether
 * to build the dump's icons. */
const started = once(async () => {
  wanted = false;
  // An event carries the whole state, so it wins over a slower load.
  const unlisten = await events.itemsEvent.listen(({ payload }) =>
    put(payload),
  );
  call(commands.itemsState())
    .then((s) => useItems.getState().loaded || put(s))
    .catch(fail);
  return unlisten;
});

export const { init, stop } = started;

/** How long option edits settle before they're saved, so dragging the
 * color picker saves once. */
const SETTLE_MS = 300;
/** The save waiting for the options to settle. */
let saving: ReturnType<typeof setTimeout> | undefined;

/** Shows `options` now and saves them once they settle, even if the tab
 * has closed by then. */
export function setOptions(options: Options) {
  useItems.setPath("options", options);
  clearTimeout(saving);
  saving = setTimeout(
    () => call(commands.itemsOptions(options)).catch(fail),
    SETTLE_MS,
  );
}

/** Lays the sections out as `sections`, saving them once they settle. */
export const setSections = (sections: Placement[]) =>
  setOptions({ ...useItems.getPath("options"), sections });
