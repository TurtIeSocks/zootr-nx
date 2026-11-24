import { describe, expect, it } from "vitest";
import { appended, applied, KEPT } from "@/stores/ap";

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

describe("a reset log", () => {
  it("drops the old seed's lines and error before the new event's line", async () => {
    const old = { lines: [{ time: 0, text: "sent" }], error: "no" };
    expect(
      applied(old, { status: "offline", line: null, clear: true }),
    ).toEqual({ status: "offline", lines: [], error: null });
  });
});
