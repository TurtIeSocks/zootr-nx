import { expect, test } from "vitest";
import { GROUP_HELP, TOPICS, type Topic } from "@/help/topics";
import {
  APP_PANES,
  PLAY_FOLDERS,
  RANDOMIZER_PANES,
  RUN_TABS,
  TOP_TABS,
} from "@/shell/nav";
import { rust } from "@/test/rust";
import { SOURCES } from "@/test/sources";

/** Every `help="…"`, `help: "…"` and `topic="…"` in the code. */
const named = Object.values(SOURCES).flatMap((text) =>
  [...text.matchAll(/\b(?:help|topic)(?:=|:\s*)"([a-z0-9-]+)"/g)].map(
    (m) => m[1],
  ),
);

/** Words a player shouldn't need to know. */
const DEV_WORDS =
  /\b(romfs|exefs|gamedata|boolexp|hash|hook|bridge|flag|nup)\b/i;

test("every topic is used somewhere, and every use names a topic", () => {
  const ids = Object.keys(TOPICS);
  expect(ids.filter((id) => !named.includes(id))).toEqual([]);
  expect(named.filter((id) => !ids.includes(id ?? ""))).toEqual([]);
});

test("topics speak plainly: one to three sentences, no developer words", () => {
  for (const [id, t] of Object.entries(TOPICS)) {
    const text = [t.title, ...t.body].join(" ");
    expect({ id, word: DEV_WORDS.exec(text)?.[0] }).toEqual({ id });
    expect({ id, dash: text.includes("—") }).toEqual({ id, dash: false });
    expect(t.body.length, id).toBeGreaterThanOrEqual(1);
    expect(t.body.length, id).toBeLessThanOrEqual(3);
  }
});

/** The schema's group names, from the Rust side. */
const schemaGroups = () => {
  const mod = /pub mod group \{([^}]*)\}/.exec(rust("settings.rs"))?.[1] ?? "";
  return [...mod.matchAll(/&str = "([^"]+)"/g)].map((m) => m[1] ?? "");
};

test("every topic's page is one the app has", () => {
  const tabs: string[] = [...TOP_TABS, ...RUN_TABS].map(([t]) => t);
  const sections = [
    ...[...APP_PANES, ...RANDOMIZER_PANES].map(([s]) => s),
    ...schemaGroups(),
  ];
  const placed = Object.entries(TOPICS as Record<string, Topic>).filter(
    ([, t]) => t.at,
  );
  expect(placed.length).toBeGreaterThan(5);
  for (const [id, { at }] of placed) {
    if (!at) continue;
    const real =
      "section" in at
        ? sections.includes(at.section)
        : tabs.includes(at.tab) &&
          (!at.folder || (at.tab === "Play" && at.folder in PLAY_FOLDERS));
    expect({ id, real }).toEqual({ id, real: true });
  }
});

test("every settings group has its page bubble", () => {
  const groups = schemaGroups();
  expect(groups.length).toBeGreaterThan(3);
  expect(groups.filter((g) => !Object.hasOwn(GROUP_HELP, g ?? ""))).toEqual([]);
  for (const [g, t] of Object.entries(GROUP_HELP))
    expect({ g, word: DEV_WORDS.exec(t.body.join(" "))?.[0] }).toEqual({ g });
});
