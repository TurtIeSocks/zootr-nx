import { act, renderHook } from "@testing-library/react";
import { expect, test } from "vitest";
import { useRequestState } from "@/lib/request";

test("a string it returns is what it said", async () => {
  const { result } = renderHook(useRequestState);
  await act(() => result.current.run(async () => "Saved"));
  expect(result.current.said).toEqual({ ok: true, text: "Saved" });
  expect(result.current.error).toBeUndefined();
});

test("anything else says nothing", async () => {
  const { result } = renderHook(useRequestState);
  await act(() => result.current.run(async () => ({ log: [] })));
  expect(result.current.said).toBeUndefined();
});

test("a throw is why it failed, until cleared", async () => {
  const { result } = renderHook(useRequestState);
  await act(() =>
    result.current.run(async () => {
      throw new Error("Disk full");
    }),
  );
  expect(result.current.said).toEqual({ ok: false, text: "Disk full" });
  expect(result.current.error).toBe("Disk full");
  act(() => result.current.clear());
  expect(result.current.error).toBeUndefined();
});

test("a second run while one is busy is ignored", async () => {
  const { result } = renderHook(useRequestState);
  let finish = () => {};
  let first!: Promise<void>;
  act(() => {
    first = result.current.run(
      () => new Promise<void>((resolve) => (finish = resolve)),
    );
  });
  expect(result.current.busy).toBe(true);
  let ran = false;
  await act(() =>
    result.current.run(async () => {
      ran = true;
    }),
  );
  expect(ran).toBe(false);
  await act(async () => {
    finish();
    await first;
  });
  expect(result.current.busy).toBe(false);
});
