import { expect, test } from "vitest";
import {
  BLANK,
  isSaved,
  problem,
  toDraft,
  toTarget,
  withTarget,
} from "@/settings/targets-draft";

const runner = { name: "Runner", app: "/a", mod_dir: "/m" };

test("a draft saves back as the target it came from, blanks left out", () => {
  expect(toTarget(toDraft(runner))).toEqual({
    ...runner,
    args: [],
    save_dir: null,
    settings_file: null,
    control: null,
  });
});

test("an added target is numbered past the names taken", () => {
  const drafts = withTarget(withTarget([toDraft(runner)], runner), runner);
  expect(drafts.map((d) => d.name)).toEqual(["Runner", "Runner 2", "Runner 3"]);
});

test("a draft is saved only while it matches its saved target", () => {
  const draft = toDraft(runner);
  expect(isSaved([runner], draft)).toBe(true);
  expect(isSaved([runner], { ...draft, app: "/b" })).toBe(false);
  expect(isSaved([], draft)).toBe(false);
});

test("problem names the first thing stopping a save", () => {
  const ok = toDraft(runner);
  expect(problem([ok])).toBe("");
  expect(problem([BLANK])).toMatch(/needs a name/);
  expect(problem([ok, ok])).toBe("Target names must differ");
  expect(problem([{ ...ok, lines: "no equals" }])).toMatch(/KEY=/);
});
