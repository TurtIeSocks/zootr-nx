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
