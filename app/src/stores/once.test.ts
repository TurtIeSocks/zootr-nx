import { expect, test, vi } from "vitest";
import { once } from "@/stores/once";

test("a start that failed runs again on the next init", async () => {
  const undo = vi.fn();
  const start = vi
    .fn()
    .mockRejectedValueOnce(new Error("no listener"))
    .mockResolvedValueOnce(undo);
  const { init, stop } = once(start);
  await expect(init()).rejects.toThrow("no listener");
  await init();
  expect(start).toHaveBeenCalledTimes(2);
  stop();
  await vi.waitFor(() => expect(undo).toHaveBeenCalledTimes(1));
});

test("stop after a failed start undoes nothing and throws nothing", async () => {
  const { init, stop } = once(() => Promise.reject(new Error("no listener")));
  const started = init();
  stop();
  await expect(started).rejects.toThrow("no listener");
});

test("a start that fails after a stop and a restart leaves the restart running", async () => {
  const undo = vi.fn();
  let fail: (e: Error) => void = () => {};
  const start = vi
    .fn()
    .mockReturnValueOnce(
      new Promise((_, reject) => {
        fail = reject;
      }),
    )
    .mockResolvedValueOnce(undo);
  const { init, stop } = once(start);
  const first = init();
  stop();
  await init();
  fail(new Error("late failure"));
  await expect(first).rejects.toThrow("late failure");
  await init();
  expect(start).toHaveBeenCalledTimes(2);
  stop();
  await vi.waitFor(() => expect(undo).toHaveBeenCalledTimes(1));
});
