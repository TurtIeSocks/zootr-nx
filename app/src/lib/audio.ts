import type { AudioDraft, Pool, PoolMode, TrackView } from "@/bindings";
import type { Change } from "@/lib/saved-sets";

/** The pools a seed can shuffle, as the pane's tabs list them. */
export const POOLS = [
  "area",
  "battle",
  "event",
  "fanfare",
  "ocarina",
  "voice-adult",
  "voice-child",
  "npc-voice",
] as const satisfies readonly Pool[];

/** A pool a seed can shuffle. */
export type ShownPool = (typeof POOLS)[number];

/** Each pool as the pane names it. */
export const POOL_LABELS: Record<ShownPool, string> = {
  area: "Area",
  battle: "Battle",
  event: "Event",
  fanfare: "Fanfare",
  ocarina: "Ocarina",
  "voice-adult": "Adult voice",
  "voice-child": "Child voice",
  "npc-voice": "NPC voices",
};

/** A track override's value for "keep this one vanilla", as the backend
 * reads it. */
export const VANILLA = "vanilla";

/** What a track's row sets it to: follow its pool, keep its own song, or
 * play a picked one. */
export type TrackMode = "pool" | "vanilla" | "pick";

/** A draft being changed: a key set to undefined is on its way out. */
interface Loose {
  pools?: Partial<Record<Pool, PoolMode | undefined>>;
  tracks?: Record<string, string | undefined>;
}

/** `draft` with its keys sorted and its empty maps left out, as the
 * backend writes it, so two drafts holding the same compare the same. */
export function tidy(draft: Loose): AudioDraft {
  const sorted = <T>(rec: Record<string, T | undefined> | undefined) =>
    Object.fromEntries(
      Object.entries(rec ?? {})
        .filter((e): e is [string, T] => e[1] !== undefined)
        // Byte order, as the backend sorts: localeCompare skips hyphens.
        .sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0)),
    );
  const pools = sorted(draft.pools);
  const tracks = sorted(draft.tracks);
  return {
    ...(Object.keys(pools).length > 0 && { pools }),
    ...(Object.keys(tracks).length > 0 && { tracks }),
  };
}

/** How many choices `draft` holds: each pool mode and track override. */
export const sizeOf = (draft: AudioDraft) =>
  Object.keys(draft.pools ?? {}).length +
  Object.keys(draft.tracks ?? {}).length;

export const poolMode = (draft: AudioDraft, pool: Pool): PoolMode =>
  draft.pools?.[pool] ?? "vanilla";

export function trackMode(draft: AudioDraft, id: string): TrackMode {
  const value = draft.tracks?.[id];
  return value === undefined ? "pool" : value === VANILLA ? "vanilla" : "pick";
}

/** Sets `pool`'s mode. Vanilla leaves the draft, as it's what an unnamed
 * pool is. */
export const setPool =
  (pool: Pool, mode: PoolMode): Change =>
  (c) => ({
    audio: tidy({
      ...c.audio,
      pools: {
        ...c.audio.pools,
        [pool]: mode === "vanilla" ? undefined : mode,
      },
    }),
  });

/** Sets track `id` to `value` (a donor's id, or {@link VANILLA}), or back
 * to its pool without one. */
export const setTrack =
  (id: string, value?: string): Change =>
  (c) => ({
    audio: tidy({ ...c.audio, tracks: { ...c.audio.tracks, [id]: value } }),
  });

/** Every pool on Shuffle. Track overrides stay. */
export const shuffleAll: Change = (c) => ({
  audio: tidy({
    ...c.audio,
    pools: Object.fromEntries(POOLS.map((p) => [p, "shuffle" as const])),
  }),
});

/** The songs `slot` may pick: its pool's tracks that give one, its own
 * left out. */
export const songsFor = (catalog: TrackView[], slot: TrackView) =>
  catalog.filter((t) => t.pool === slot.pool && t.song && t.id !== slot.id);

/** The track whose song `slot`'s play button plays: its pick, else its
 * own. Null when it's vanilla and its music is in sections. */
export function heard(draft: AudioDraft, slot: TrackView): string | null {
  const value = draft.tracks?.[slot.id];
  if (value !== undefined && value !== VANILLA) return value;
  return slot.song ? slot.id : null;
}

/** `secs` as `m:ss`. */
export const clock = (secs: number) => {
  const s = Math.round(secs);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
};

/** How long a track runs, in words: `0:12`, or `1:41 loop`. */
export const length = (t: TrackView) =>
  t.looping ? `${clock(t.ms / 1000)} loop` : clock(t.ms / 1000);

/** What a one-shot of each pool plays over. */
const MOMENTS: Partial<Record<Pool, string>> = {
  fanfare: "the get",
  ocarina: "the song",
};

/** What playing `donor` in `slot` does that its own song doesn't, when
 * the donor is outside the slot's band: it runs on past the moment, ends
 * early, or loops. Nothing in the same band or a pool without bands. */
export function bandNote(slot: TrackView, donor: TrackView): string | null {
  if (slot.band_label === "-" || donor.band === slot.band) return null;
  const moment = MOMENTS[slot.pool] ?? "the scene";
  if (donor.looping) return `Loops on past ${moment}`;
  if (slot.looping) return "Plays once, then stops";
  const gap = Math.round((donor.ms - slot.ms) / 1000);
  if (gap <= 0) return `Ends ${-gap} s before ${moment} does`;
  // A played song is cut off when its scene ends (heard live), unlike
  // music, which plays on.
  return slot.pool === "ocarina"
    ? `Runs ${gap} s longer than the song's scene, so it may be cut off`
    : `Plays on ${gap} s past ${moment}`;
}
