import { describe, expect, it } from "vitest";
import { fitTo, resize } from "@/live/use-columns";

// Padding, gaps and Change's minimum take 168px, so these need 496.
const W = { time: 48, name: 200, type: 80 };

describe("fitTo", () => {
  it("leaves widths that fit", () => {
    expect(fitTo(W, 500)).toEqual(W);
  });

  it("takes from Name first", () => {
    expect(fitTo(W, 450)).toEqual({ ...W, name: 154 });
  });

  it("then Type and Time, never below their minimums", () => {
    expect(fitTo(W, 300)).toEqual({ time: 40, name: 100, type: 40 });
  });

  it("changes nothing before the log is measured", () => {
    expect(fitTo(W, 0)).toEqual(W);
  });
});

describe("resize", () => {
  it("takes from the column to its right first", () => {
    expect(resize("name", 300, W, 500)).toEqual({
      time: 48,
      name: 244,
      type: 40,
    });
  });

  it("widens Name past a wide Type when Change is at its minimum", () => {
    const wide = { time: 48, name: 100, type: 180 };
    expect(resize("name", 200, wide, 496)).toEqual({
      time: 48,
      name: 200,
      type: 80,
    });
  });

  it("takes from Name before Type when Time grows", () => {
    expect(resize("time", 100, W, 500)).toEqual({
      time: 100,
      name: 152,
      type: 80,
    });
  });

  it("stops where Change keeps its minimum", () => {
    expect(resize("type", 999, W, 500)).toEqual({ ...W, type: 84 });
  });

  it("stops at the column's own minimum", () => {
    expect(resize("name", 10, W, 500)).toEqual({ ...W, name: 100 });
  });

  it("gives back what it took when resized back from the same start", () => {
    const grown = resize("time", 100, W, 500);
    expect(grown.name).toBe(152);
    expect(resize("time", 48, W, 500)).toEqual(W);
  });

  it("has no upper limit before the log is measured", () => {
    expect(resize("name", 999, W, 0)).toEqual({ ...W, name: 999 });
  });
});
