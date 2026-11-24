import { expect, test } from "vitest";
import { type Quests, stepOf, stepsOf } from "@/lib/quest-step";

const quests: Quests = { FakeQuest: ["Start", "Done"] };

test("a quest's steps are its own", () => {
  expect(stepsOf(quests, "FakeQuest")).toEqual(["Start", "Done"]);
  expect(stepsOf(quests, "FakeOther")).toBeUndefined();
});

test("a name off the prototype is no quest", () => {
  for (const name of ["constructor", "hasOwnProperty", "isPrototypeOf"]) {
    expect(stepsOf(quests, name)).toBeUndefined();
    expect(stepOf(quests, { challenge: name, step: "" })).toBe("");
  }
});

test("the picked step if the quest has it, else its first", () => {
  expect(stepOf(quests, { challenge: " FakeQuest ", step: "Done" })).toBe(
    "Done",
  );
  expect(stepOf(quests, { challenge: "FakeQuest", step: "Gone" })).toBe(
    "Start",
  );
});
