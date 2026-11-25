import { expect, test } from "vitest";
import type { TrackView } from "@/bindings";
import {
  bandNote,
  heard,
  length,
  resetAll,
  setPool,
  setTrack,
  shuffleAll,
  songsFor,
  tidy,
  trackMode,
} from "@/lib/audio";
import { full } from "@/lib/config";
import { savedSets } from "@/lib/saved-sets";

const track = (
  id: string,
  pool: TrackView["pool"],
  ms: number,
  more: Partial<TrackView> = {},
): TrackView => ({
  id,
  label: id,
  pool,
  band: 0,
  band_label: "-",
  ms,
  looping: true,
  song: true,
  gives_none: null,
  ...more,
});

const small = track("item-get-small", "fanfare", 2000, {
  band: 1,
  band_label: "<5s",
  looping: false,
});
const medal = track("medal-get", "fanfare", 11700, {
  band: 2,
  band_label: "5-12s",
  looping: false,
});
const heart = track("heart-get", "fanfare", 3000, {
  band: 1,
  band_label: "<5s",
  looping: false,
});
const market = track("market", "area", 60000, { song: false });
const zora = track("zora-domain", "area", 80000);

test("a pool or track set back to vanilla leaves the draft", () => {
  let c = full({});
  c = { ...c, ...setPool("area", "shuffle")(c) };
  c = { ...c, ...setTrack("market", "zora-domain")(c) };
  expect(c.audio).toEqual({
    pools: { area: "shuffle" },
    tracks: { market: "zora-domain" },
  });
  c = { ...c, ...setPool("area", "vanilla")(c) };
  c = { ...c, ...setTrack("market")(c) };
  expect(c.audio).toEqual({});
});

test("Shuffle All keeps track overrides; Reset All clears everything", () => {
  const c = full({ audio: { tracks: { market: "vanilla" } } });
  expect(shuffleAll(c).audio).toEqual({
    pools: {
      area: "shuffle",
      battle: "shuffle",
      event: "shuffle",
      fanfare: "shuffle",
      ocarina: "shuffle",
    },
    tracks: { market: "vanilla" },
  });
  expect(resetAll(c).audio).toEqual({});
});

test("a tidied draft sorts as the backend does", () => {
  const draft = tidy({ tracks: { "goron-city": "x", goroncity: "y", a: "z" } });
  expect(Object.keys(draft.tracks ?? {})).toEqual([
    "a",
    "goron-city",
    "goroncity",
  ]);
});

test("a row's mode and what its play button plays", () => {
  const draft = { tracks: { market: "zora-domain", "heart-get": "vanilla" } };
  expect(trackMode(draft, "market")).toBe("pick");
  expect(trackMode(draft, "heart-get")).toBe("vanilla");
  expect(trackMode(draft, "zora-domain")).toBe("pool");
  expect(heard(draft, market)).toBe("zora-domain");
  expect(heard({}, market)).toBeNull();
  expect(heard({}, zora)).toBe("zora-domain");
});

test("a pick lists its pool's songs, its own and sections left out", () => {
  const catalog = [small, medal, heart, market, zora];
  expect(songsFor(catalog, small).map((t) => t.id)).toEqual([
    "medal-get",
    "heart-get",
  ]);
  expect(songsFor(catalog, zora)).toEqual([]);
});

test("a song outside the slot's band says what it does", () => {
  expect(bandNote(small, heart)).toBeNull();
  expect(bandNote(small, medal)).toBe("Plays on 10 s past the get");
  expect(bandNote(medal, small)).toBe("Ends 10 s before the get does");
  expect(bandNote(zora, market)).toBeNull();
  const taught = track("taught-suns-song", "ocarina", 3560, {
    band: 1,
    band_label: "<7s",
    looping: false,
  });
  const warp = track("played-requiem-of-spirit", "ocarina", 21260, {
    band: 3,
    band_label: ">14s",
    looping: false,
  });
  expect(bandNote(taught, warp)).toBe(
    "Runs 18 s longer than the song's scene, so it may be cut off",
  );
  expect(bandNote(warp, taught)).toBe("Ends 18 s before the song does");
  expect(length(small)).toBe("0:02");
  expect(length(zora)).toBe("1:20 loop");
});

test("an audio set counts its pools and tracks", () => {
  const sets = savedSets("audio");
  const c = full({
    audio: { pools: { area: "shuffle" }, tracks: { market: "vanilla" } },
    audio_sets: {
      mine: { pools: { area: "shuffle" }, tracks: { market: "vanilla" } },
    },
  });
  expect(sets.size(c.audio)).toBe(2);
  expect(sets.savedAs(c)).toBe("mine");
  expect(sets.unsaved(full({ audio: {} }))).toBe(false);
});
