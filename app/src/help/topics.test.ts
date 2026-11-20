import { expect, test } from "vitest";
import { TOPICS } from "@/help/topics";
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
