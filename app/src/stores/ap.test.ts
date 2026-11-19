import { describe, expect, it } from "vitest";
import { appended, KEPT } from "@/stores/ap";

describe("the client log", () => {
  it("keeps only the newest lines, as many as the client keeps", () => {
    const line = (n: number) => ({ time: n, text: `sent ${n}` });
    const full = Array.from({ length: KEPT }, (_, n) => line(n));
    const next = appended(full, line(KEPT));
    expect(next).toHaveLength(KEPT);
    expect(next[0]).toEqual(line(1));
    expect(next.at(-1)).toEqual(line(KEPT));
    expect(appended([], line(0))).toEqual([line(0)]);
  });
});
