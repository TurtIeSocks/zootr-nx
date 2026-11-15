import { expect, test } from "vitest";
import { matching } from "@/lib/matching";

test("matching needs a query and stops at the limit", () => {
  const names = ["Sequence_IsChildhood", "Sequence_CurrentBanc", "Other"];
  expect(matching(names, "")).toEqual([]);
  expect(matching(names, "sequence", 1)).toEqual(["Sequence_IsChildhood"]);
});
