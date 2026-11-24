import { expect, test } from "vitest";
import {
  gameLost,
  keeps,
  kindOf,
  reconnecting,
  refusal,
} from "@/play/ap-lines";

const at = (...texts: string[]) => texts.map((text) => ({ time: 0, text }));

test("a note's kind comes from how it starts", () => {
  expect(kindOf("Link died, DeathLink sent")).toBe("death");
  expect(kindOf("refused: x")).toBe("refused");
  expect(kindOf("connected")).toBe("room");
  expect(kindOf("the game closed")).toBe("trouble");
});

test("Items keeps sent and received, Room the rest", () => {
  expect(keeps("items", "received")).toBe(true);
  expect(keeps("items", "death")).toBe(false);
  expect(keeps("room", "sent")).toBe(false);
  expect(keeps("room", "trouble")).toBe(true);
  expect(keeps("all", "sent")).toBe(true);
});

test("a refusal names the slot, or keeps the server's words as a sentence", () => {
  const no = "refused: Archipelago refused connection: ";
  expect(
    refusal(
      at(`${no}the name provided doesn't match any names on the server`),
      "FakeSlot",
    ),
  ).toBe("The room has no slot named FakeSlot.");
  expect(refusal(at(`${no}invalid ItemsHandling flag`), "FakeSlot")).toBe(
    "Invalid ItemsHandling flag.",
  );
  expect(refusal([], "FakeSlot")).toBe("The room refused the connection.");
});

test("reconnecting while the last note is a lost connection", () => {
  expect(reconnecting(at("connected", "lost (x), retrying in 5 s"))).toBe(true);
  expect(reconnecting(at("lost (x), retrying in 5 s", "connecting"))).toBe(
    false,
  );
});

test("the game is lost until the client says it answers again", () => {
  const lost = "can't reach the game: Broken pipe (os error 32)";
  expect(gameLost(at("connected", lost))).toBe(true);
  expect(gameLost(at(lost, "sent Fake Chest"))).toBe(true);
  expect(gameLost(at(lost, "the game answers again"))).toBe(false);
  expect(gameLost(at("connected"))).toBe(false);
});
