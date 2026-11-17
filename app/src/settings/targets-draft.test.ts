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
    bridge: null,
    env: [],
    harness_env: [],
    rebuild: null,
  });
});

test("a rebuild command keeps one argument per line", () => {
  const baked = {
    ...runner,
    rebuild: {
      command: ["bash", "-c", "make game --update"],
      manifest: "/g/exefs_patches.txt",
    },
  };
  const draft = toDraft(baked);
  expect(draft.rebuild_command).toBe("bash\n-c\nmake game --update");
  expect(toTarget(draft)).toMatchObject(baked);
  expect(toTarget({ ...draft, manifest: " " }).rebuild).toEqual({
    command: baked.rebuild.command,
    manifest: null,
  });
});

test("a bridge and its env lines save back as they were", () => {
  const native = {
    ...runner,
    bridge: "/run/c.sock",
    env: ["SOCK={bridge}", "MEM=rw"],
    harness_env: ["SAVES={save}", "FPS=30"],
  };
  expect(toTarget(toDraft(native))).toMatchObject(native);
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
  expect(problem([{ ...ok, env: "no equals" }])).toMatch(/NAME=VALUE/);
  expect(problem([{ ...ok, env: "S={bridge}" }])).toMatch(/control socket/);
  expect(problem([{ ...ok, env: "S={bridge}", bridge: "/s" }])).toBe("");
  expect(problem([{ ...ok, harness_env: "no equals" }])).toMatch(/NAME=VALUE/);
  expect(problem([{ ...ok, harness_env: "S={bridge}" }])).toMatch(
    /control socket/,
  );
});
