import { expect, test } from "vitest";
import { matching } from "@/lib/matching";

test("matching needs a query and stops at the limit", () => {
  const names = ["FakeFlag_IsChild", "FakeFlag_CurrentMap", "Other"];
  expect(matching(names, "")).toEqual([]);
  expect(matching(names, "flag", 1)).toEqual(["FakeFlag_IsChild"]);
});
