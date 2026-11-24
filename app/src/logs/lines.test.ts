import { expect, test } from "vitest";
import type { Level, Line } from "@/bindings";
import { add, CAP, EMPTY, textOf, visible } from "@/logs/lines";

const line = (seq: number, level: Level = "info", text = `l${seq}`): Line => ({
  seq,
  time: "12:00:00",
  level,
  text,
});

test("add skips lines it already holds and keeps the newest", () => {
  let log = add(EMPTY, [line(0), line(1)]);
  log = add(log, [line(1), line(2)]);
  expect(log.lines.map((l) => l.id)).toEqual([0, 1, 2]);
  expect(log.next).toBe(3);
  log = add(
    log,
    Array.from({ length: CAP }, (_, i) => line(i + 3)),
  );
  expect(log.lines).toHaveLength(CAP);
  expect(log.lines[0]?.seq).toBe(3);
});

test("a line sent out of order slots in, once", () => {
  let log = add(EMPTY, [line(0), line(2)]);
  log = add(log, [line(1)]);
  expect(log.lines.map((l) => l.id)).toEqual([0, 1, 2]);
  expect(log.next).toBe(3);
  expect(add(log, [line(1)])).toBe(log);
});

test("debug shows only with developer tools, and the filter and levels narrow", () => {
  const lines = add(EMPTY, [
    line(0, "error", "Install failed"),
    line(1, "debug", "→ status 0.01 s"),
    line(2, "info", "Game: waiting"),
  ]).lines;
  const texts = (ls: Line[]) => ls.map((l) => l.text);
  expect(texts(visible(lines, "", [], false))).toEqual([
    "Install failed",
    "Game: waiting",
  ]);
  expect(visible(lines, "", [], true)).toHaveLength(3);
  expect(texts(visible(lines, "GAME", [], true))).toEqual(["Game: waiting"]);
  expect(texts(visible(lines, "", ["info", "debug"], true))).toEqual([
    "Install failed",
  ]);
});

test("copied lines read like the file", () => {
  expect(textOf([line(0, "warn", "x"), line(1, "error", "y")])).toBe(
    "12:00:00  warn   x\n12:00:00  error  y",
  );
});
