import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, test } from "vitest";
import { useLoad } from "@/lib/use-load";

test("loads, then says what it got", async () => {
  const load = async (n: number) => n * 2;
  const { result } = renderHook(() => useLoad(load, 2));
  expect(result.current).toEqual({ status: "loading" });
  await waitFor(() =>
    expect(result.current).toEqual({ status: "ok", data: 4 }),
  );
});

test("says why a load failed", async () => {
  const load = async (_: null) => {
    throw new Error("No saves");
  };
  const { result } = renderHook(() => useLoad(load, null));
  await waitFor(() =>
    expect(result.current).toEqual({ status: "error", error: "No saves" }),
  );
});

test("drops an answer for a key that has changed", async () => {
  const answers = new Map<string, (s: string) => void>();
  const load = (key: string) =>
    new Promise<string>((resolve) => answers.set(key, resolve));
  const { result, rerender } = renderHook(({ key }) => useLoad(load, key), {
    initialProps: { key: "old" },
  });
  rerender({ key: "new" });
  await act(async () => answers.get("new")?.("new data"));
  await act(async () => answers.get("old")?.("old data"));
  expect(result.current).toEqual({ status: "ok", data: "new data" });
});
