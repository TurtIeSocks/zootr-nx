import { expect, test } from "vitest";
import {
  BLANK,
  importedDraft,
  isSaved,
  namesVariable,
  problem,
  toDraft,
  toTarget,
  VARIABLES,
} from "@/settings/targets-draft";
import { rust } from "@/test/rust";

const fake = { name: "FakeTarget", app: "/a", mod_dir: "/m" };

test("a draft saves back as the target it came from, blanks left out", () => {
  expect(toTarget(toDraft(fake))).toEqual({
    ...fake,
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
    ...fake,
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

test("a launch argument holding a space saves back whole", () => {
  const spaced = { ...fake, args: ["--rom", "~/My Games/{game}"] };
  const draft = toDraft(spaced);
  expect(draft.args).toBe("--rom\n~/My Games/{game}");
  expect(toTarget(draft)).toMatchObject(spaced);
});

test("a bridge and its env lines save back as they were", () => {
  const native = {
    ...fake,
    bridge: "/run/c.sock",
    env: ["SOCK={bridge}", "MEM=rw"],
    harness_env: ["SAVES={save}", "FPS=30"],
  };
  expect(toTarget(toDraft(native))).toMatchObject(native);
});

test("an added target is numbered past the names taken", () => {
  const drafts = [toDraft(fake)];
  drafts.push(importedDraft(drafts, fake));
  drafts.push(importedDraft(drafts, fake));
  expect(drafts.map((d) => d.name)).toEqual([
    "FakeTarget",
    "FakeTarget 2",
    "FakeTarget 3",
  ]);
});

test("a draft is saved only while it matches its saved target", () => {
  const draft = toDraft(fake);
  expect(isSaved([fake], draft)).toBe(true);
  expect(isSaved([fake], { ...draft, app: "/b" })).toBe(false);
  expect(isSaved([], draft)).toBe(false);
});

test("problem names the first thing stopping a save", () => {
  const ok = toDraft(fake);
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

test("Variables lists exactly the variables the code fills", () => {
  const filled = [
    "config.rs",
    "config/settings_file.rs",
    "config/rebuild.rs",
  ].flatMap((f) => [...rust(f).matchAll(/"(\{[a-z]+\})"/g)].map((m) => m[1]));
  expect(new Set(filled)).toEqual(new Set(VARIABLES.map((v) => v.name)));
});

test("a draft names a variable when any of its fields holds one", () => {
  expect(namesVariable(toDraft(fake))).toBe(false);
  expect(namesVariable({ ...toDraft(fake), env: "SOCK={bridge}" })).toBe(true);
});
