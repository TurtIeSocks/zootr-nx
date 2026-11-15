import { expect, test } from "vitest";
import type { Change, Event, Session } from "@/bindings";
import {
  type Action,
  CAP,
  fold,
  IDLE,
  isRow,
  kindsOf,
  type Live,
  lineOf,
  reduce,
  textOf,
  visible,
} from "@/live/rows";

const change = (hash: number, name: string, label: string | null = null) =>
  ({ hash, name, label, kind: "Int", change: "1 -> 2" }) satisfies Change;

let seq = 0;
const ev = (event: Event): Action => ({ type: "Event", seq: seq++, event });
const row = (c: Change) => ev({ type: "Row", at: 1, save: false, change: c });
const rows = (s: Live) => s.items.filter(isRow);

const session: Session = {
  source: "Live",
  reason: null,
  log: "/l.log",
  unsaved: null,
  started: "22:51",
};

test("keeps the newest rows on screen and counts the rest", () => {
  let s: Live = IDLE;
  for (let i = 0; i < CAP + 3; i++) s = reduce(s, row(change(i, `V${i}`)));
  expect(s.items).toHaveLength(CAP);
  expect(s.dropped).toBe(3);
  expect(rows(s)[0]?.change.name).toBe("V3");
  s = reduce(s, { type: "Cleared" });
  expect([s.items.length, s.dropped]).toEqual([0, 0]);
});

test("a label reaches every row with that hash, and empty removes it", () => {
  let s = [change(7, "#00000007"), change(7, "#00000007[1]"), change(8, "A")]
    .map(row)
    .reduce(reduce, IDLE);
  s = reduce(s, { type: "Labelled", hash: 7, label: "door" });
  expect(rows(s).map((r) => r.change.label)).toEqual(["door", "door", null]);
  s = reduce(s, { type: "Labelled", hash: 7, label: "" });
  expect(rows(s)[0]?.change.label).toBeNull();
});

test("quiet keeps named and labelled values, grep matches any field", () => {
  const started = reduce(IDLE, ev({ type: "Started", session }));
  const s = [
    change(0xab, "PlayerStatus.CurrentRupee"),
    change(0x9824206c, "#9824206c", "door state"),
    change(0xe573f564, "#e573f564"),
  ]
    .map(row)
    .reduce(reduce, started);
  const names = (quiet: boolean, grep: string) =>
    visible(s.items, quiet, grep).map((i) =>
      isRow(i) ? i.change.name : "note",
    );
  expect(names(true, "")).toEqual([
    "note",
    "PlayerStatus.CurrentRupee",
    "#9824206c",
  ]);
  expect(names(false, "")).toHaveLength(4);
  expect(names(false, "DOOR")).toEqual(["note", "#9824206c"]);
  expect(names(false, "e573")).toEqual(["note", "#e573f564"]);
});

test("sessions leave notes, and muted values reset at each start", () => {
  const s = [
    ev({ type: "Started", session }),
    ev({ type: "Muted", hash: 1, name: "Noisy", manual: false }),
    ev({ type: "Ended", reason: "the game closed" }),
    ev({ type: "Started", session: { ...session, started: "23:10" } }),
  ].reduce(reduce, IDLE);
  const notes = s.items.map((i) => ("note" in i ? i.note : ""));
  expect(notes).toEqual([
    "Session started 22:51",
    "Session ended: the game closed",
    "Session started 23:10",
  ]);
  expect(s.muted).toEqual([]);
  expect(s.session?.started).toBe("23:10");
});

test("a snapshot and the events around it come out once each", () => {
  const n = (seq: number, hash: number) => ({
    seq,
    event: {
      type: "Row" as const,
      at: 1,
      save: false,
      change: change(hash, `V${hash}`),
    },
  });
  const snapshot = {
    status: { state: "Recording" as const, session },
    events: [n(0, 0), n(1, 1)],
    dropped: 0,
    next: 2,
  };
  let s = reduce(IDLE, { type: "Snapshot", snapshot });
  for (const e of [n(1, 1), n(2, 2)]) s = reduce(s, { type: "Event", ...e });
  expect(rows(s).map((r) => r.id)).toEqual([0, 1, 2]);
  expect([s.status.state, s.next]).toEqual(["Recording", 3]);
});

test("a line is time, name, kind and change, tab separated", () => {
  const plain = { id: 1, at: 75, save: false, change: change(1, "Rupee") };
  expect(lineOf(plain)).toBe("01:15\tRupee\tInt\t1 -> 2");
  const saved = {
    ...plain,
    save: true,
    change: change(2, "#00000002", "gate"),
  };
  expect(lineOf(saved)).toBe("01:15\tsave #00000002 (gate)\tInt\t1 -> 2");
  const note = { id: 2, note: "Session started 22:51", tone: "start" as const };
  expect(lineOf(note)).toBe("--- Session started 22:51 ---");
  expect(textOf([plain, note])).toBe(`${lineOf(plain)}\n${lineOf(note)}`);
});

test("fold merges neighbours of one value and source only", () => {
  const r = (id: number, hash: number, text: string, save = false) => ({
    id,
    at: id,
    save,
    change: { ...change(hash, `v${hash}`), change: text },
  });
  const note = { id: 9, note: "n", tone: "start" as const };
  const folded = fold([
    r(1, 1, "1 -> 2"),
    r(2, 1, "2 -> 3"),
    r(3, 1, "3 -> 4"),
    r(4, 1, "4 -> 5", true),
    r(5, 2, "a -> b"),
    note,
    r(6, 2, "b -> c"),
    r(7, 2, "grew"),
  ]);
  expect(folded.map(lineOf)).toEqual([
    "00:03\tv1\tInt\t1 -> 4 ×3",
    "00:04\tsave v1\tInt\t4 -> 5",
    "00:05\tv2\tInt\ta -> b",
    "--- n ---",
    "00:07\tv2\tInt\tgrew ×2",
  ]);
  expect(folded[0]?.id).toBe(3);
});

test("hidden kinds leave the log, and stay listed to bring back", () => {
  const int = { id: 1, at: 1, save: false, change: change(1, "a") };
  const bool = { ...int, id: 2, change: { ...change(2, "b"), kind: "Bool" } };
  expect(visible([int, bool], false, "", ["Bool"])).toEqual([int]);
  expect(kindsOf([int], ["Bool"])).toEqual(["Bool", "Int"]);
});

test("a hand mute drops the value's rows, and a marker joins the log", () => {
  const s = [
    row(change(1, "Noisy")),
    row(change(2, "Calm")),
    ev({ type: "Muted", hash: 2, name: "Calm", manual: false }),
    ev({ type: "Muted", hash: 1, name: "Noisy", manual: true }),
    ev({ type: "Marked", at: 75, text: "opened the chest" }),
  ].reduce(reduce, IDLE);
  expect(rows(s).map((r) => r.change.name)).toEqual(["Calm"]);
  expect(s.muted.map((m) => m.name)).toEqual(["Calm", "Noisy"]);
  expect(s.items.at(-1)).toMatchObject({
    note: "01:15 · opened the chest",
    tone: "mark",
  });
});
