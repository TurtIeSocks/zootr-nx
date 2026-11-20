import { expect, test } from "vitest";
import { errorParts } from "@/lib/api";

test("an error splits at its first blank line into words and details", () => {
  expect(errorParts("Choose the game\n\nopening a.nup: bad\n\nmore")).toEqual({
    said: "Choose the game",
    details: "opening a.nup: bad\n\nmore",
  });
  expect(errorParts("opening a.nup: bad")).toEqual({
    said: "opening a.nup: bad",
  });
});
