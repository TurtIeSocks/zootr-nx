import { useEffect, useMemo } from "react";
import { commands, type DevNear } from "@/bindings";
import { matches, tag } from "@/dev/tag";
import { call, message } from "@/lib/api";
import { count } from "@/lib/format";
import { type HiddenActor, type LiveRead, useDev } from "@/stores/dev";

/** How often Follow reads again. */
export const FOLLOW_MS = 2000;
/** How many more tries a re-hide gets for actors not made yet, and how far
 * apart: a map's actors come in over its first moments. */
export const RETRIES = 4;
export const RETRY_MS = 1000;

/** Reads so far, so a read a later one overtook is dropped. A hide or show
 * counts as one too, so a read it overtook can't undo its marks. */
let reads = 0;

/** Reads the live actors again, nearest Link first, or why they can't be,
 * and hides again the listed ones it finds shown. */
export async function readLive(): Promise<void> {
  const map = useDev.getPath("game.state.map");
  if (!map) return;
  const mine = ++reads;
  let read: LiveRead;
  try {
    read = {
      status: "ok",
      map,
      list: await call(commands.devNearby()),
      at: Date.now(),
    };
  } catch (e) {
    read = { status: "error", error: message(e) };
  }
  if (mine !== reads) return;
  useDev.setPath("live.read", read);
  if (read.status === "ok") await hideAgain(read.map, read.list);
}

/** Hides again the listed actors of `map` found shown: made again since
 * their hide, as Link walks back into a room of the same map. */
async function hideAgain(map: string, list: DevNear[]): Promise<void> {
  const listed = new Set(
    useDev
      .getPath("live.hidden")
      .filter((h) => h.map === map)
      .map((h) => h.hash),
  );
  const shown = list
    .filter((n) => n.hidden === "shown" && listed.has(n.hash))
    .map((n) => n.hash);
  if (shown.length === 0) return;
  mark(await hideQuietly(shown), "ours");
}

/** Hides these placements, those it found, or none on a refusal. */
const hideQuietly = (hashes: string[]) =>
  call(commands.devHide(hashes, true)).catch(() => [] as string[]);

/** The placements `read` found hidden by us. */
function oursOf(read: LiveRead): Set<string> {
  return new Set(
    read.status === "ok"
      ? read.list.filter((n) => n.hidden === "ours").map((n) => n.hash)
      : [],
  );
}

/** Marks these placements' rows as `hidden`, leaving the order alone. */
function mark(hashes: string[], hidden: DevNear["hidden"]) {
  const read = useDev.getPath("live.read");
  if (read.status !== "ok" || hashes.length === 0) return;
  const marked = new Set(hashes);
  const list = read.list.map((n) =>
    marked.has(n.hash) ? { ...n, hidden } : n,
  );
  useDev.setPath("live.read", { ...read, list });
}

/** The hidden list without these placements. */
function without(hashes: string[]): HiddenActor[] {
  return useDev.getPath("live.hidden").filter((h) => !hashes.includes(h.hash));
}

/** An actor and what tells it from others of its name. */
export const named = (actor: string, tag: string | null) =>
  tag ? `${actor} ${tag}` : actor;

/** Hides `near`'s model, or shows it again, keeping the hidden list.
 * `tag` tells it from others of its name in what it says. */
export async function toggle(
  near: DevNear,
  map: string,
  tag: string | null,
): Promise<string> {
  const on = near.hidden === "shown";
  const found = await call(commands.devHide([near.hash], on));
  reads++;
  if (!found.includes(near.hash)) {
    throw new Error(`${near.actor} isn't live anymore. Refresh the list`);
  }
  mark([near.hash], on ? "ours" : "shown");
  const { hash, actor, label } = near;
  const rest = without([hash]);
  useDev.setPath(
    "live.hidden",
    on ? [...rest, { hash, actor, label, map }] : rest,
  );
  return `${on ? "Hid" : "Showed"} ${named(actor, tag)} in ${map}`;
}

/** Shows these hidden actors again, those on Link's map in the game, and
 * drops them from the list. */
async function show(gone: HiddenActor[]): Promise<void> {
  const map = useDev.getPath("game.state.map");
  const here = gone.filter((h) => h.map === map).map((h) => h.hash);
  if (here.length > 0) {
    const found = await call(commands.devHide(here, false));
    reads++;
    mark(found, "shown");
  }
  useDev.setPath("live.hidden", without(gone.map((h) => h.hash)));
}

/** Shows one hidden actor again. */
export async function showOne(
  hidden: HiddenActor,
  tag: string | null,
): Promise<string> {
  await show([hidden]);
  return `Showed ${named(hidden.actor, tag)} in ${hidden.map}`;
}

/** Shows every hidden actor again. */
export async function showAll(): Promise<string> {
  const all = useDev.getPath("live.hidden");
  await show(all);
  return `Showed ${count(all.length, "actor")}`;
}

const wait = (ms: number) => new Promise((done) => setTimeout(done, ms));

/** Hides `map`'s listed actors again after a load, trying again for those
 * not made yet. Stops once Link leaves the map. */
async function rehide(map: string): Promise<void> {
  let pending = useDev.getPath("live.hidden").filter((h) => h.map === map);
  for (let tries = 0; pending.length > 0 && tries <= RETRIES; tries++) {
    if (tries > 0) await wait(RETRY_MS);
    if (useDev.getPath("game.state.map") !== map) return;
    // One shown from the strip meanwhile is no longer wanted, and one a
    // read hid again needs nothing more.
    const listed = new Set(useDev.getPath("live.hidden").map((h) => h.hash));
    const ours = oursOf(useDev.getPath("live.read"));
    pending = pending.filter((h) => listed.has(h.hash) && !ours.has(h.hash));
    const hashes = pending.map((h) => h.hash);
    if (hashes.length === 0) return;
    const found = await hideQuietly(hashes);
    mark(found, "ours");
    pending = pending.filter((h) => !found.includes(h.hash));
  }
}

/** Reads once the window shows and after each finished map load, hiding
 * the map's listed actors again, and every `FOLLOW_MS` while Follow is on.
 * Call once, from the window, so a load hides them again on any tab. */
export function useLiveReads() {
  const [map] = useDev.usePath("game.state.map");
  const [loading] = useDev.usePath("game.state.loading");
  const [follow] = useDev.usePath("live.follow");

  useEffect(() => {
    if (!map || loading) return;
    void readLive().then(() => rehide(map));
  }, [map, loading]);

  useEffect(() => {
    if (!follow) return;
    const id = setInterval(() => void readLive(), FOLLOW_MS);
    return () => clearInterval(id);
  }, [follow]);
}

/** A row of the list: the actor and what tells it from others of its
 * name. */
export interface LiveRow {
  near: DevNear;
  tag: string | null;
}

/** The last read, its rows matching the search, in distance order. Tags
 * count over every row, so a search keeps each one's `#n`. */
export function useLiveRows():
  | { status: "ok"; map: string; rows: LiveRow[]; at: number }
  | Exclude<LiveRead, { status: "ok" }> {
  const [read] = useDev.usePath("live.read");
  const [query] = useDev.usePath("live.query");

  return useMemo(() => {
    if (read.status !== "ok") return read;
    const rows = read.list
      .map((near) => ({ near, tag: tag(read.list, near) }))
      .filter(({ near }) => matches(near, query));
    return { status: "ok", map: read.map, rows, at: read.at };
  }, [read, query]);
}

/** A chip of the hidden strip: the actor, its tag, and whether it's hidden
 * in the game now (on Link's map, and found there). */
export interface HiddenChip {
  hidden: HiddenActor;
  tag: string | null;
  live: boolean;
}

/** The hidden list as the strip shows it. */
export function useHiddenChips(): HiddenChip[] {
  const [hidden] = useDev.usePath("live.hidden");
  const [read] = useDev.usePath("live.read");

  return useMemo(() => {
    const ours = oursOf(read);
    const map = read.status === "ok" ? read.map : null;
    return hidden.map((h) => ({
      hidden: h,
      tag: tag(hidden, h),
      live: h.map === map && ours.has(h.hash),
    }));
  }, [hidden, read]);
}
