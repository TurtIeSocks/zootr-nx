import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { useCopied } from "@/lib/use-copied";

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

test("says what was copied for a moment", () => {
  const { result } = renderHook(useCopied);
  act(() => result.current.flash("Kakariko"));
  expect(result.current.copied).toBe("Kakariko");
  act(() => vi.advanceTimersByTime(1499));
  expect(result.current.copied).toBe("Kakariko");
  act(() => vi.advanceTimersByTime(1));
  expect(result.current.copied).toBeUndefined();
});

test("a second copy restarts the moment", () => {
  const { result } = renderHook(useCopied);
  act(() => result.current.flash("a"));
  act(() => vi.advanceTimersByTime(1000));
  act(() => result.current.flash("b"));
  act(() => vi.advanceTimersByTime(1000));
  expect(result.current.copied).toBe("b");
});

test("leaves no timer behind on unmount", () => {
  const { result, unmount } = renderHook(useCopied);
  act(() => result.current.flash("a"));
  unmount();
  expect(vi.getTimerCount()).toBe(0);
});
