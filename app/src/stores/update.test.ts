import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { full } from "@/lib/config";
import { useConfig } from "@/stores/config";
import { init, stop, useUpdate } from "@/stores/update";

vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "0.1.0" }));
vi.mock("@/stores/config", async (original) => ({
  ...(await original<typeof import("@/stores/config")>()),
  init: async () => {},
}));

const release = (tag: string) => ({
  ok: true,
  json: async () => ({ tag_name: tag, html_url: "https://github.com/o/r" }),
});

beforeEach(() => {
  vi.stubEnv("VITE_UPDATE_REPO", "o/r");
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(release("v0.2.0")));
  useConfig.setPath("config", full({}));
});

afterEach(() => {
  stop();
  useUpdate.setState({ latest: null, current: null });
  vi.useRealTimers();
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

test("a newer release is kept, with this build's version", async () => {
  await init();
  expect(useUpdate.getState().latest?.version).toBe("0.2.0");
  expect(useUpdate.getState().current).toBe("0.1.0");
});

test("an older or equal release isn't", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(release("v0.1.0")));
  await init();
  expect(useUpdate.getState().latest).toBeNull();
});

test("the switch off makes no request", async () => {
  useConfig.setPath("config", full({ no_update_check: true }));
  await init();
  expect(fetch).not.toHaveBeenCalled();
});

test("dev without VITE_UPDATE_REPO makes no request", async () => {
  vi.stubEnv("VITE_UPDATE_REPO", "");
  await init();
  expect(fetch).not.toHaveBeenCalled();
});

test("it checks again after 6 hours", async () => {
  vi.useFakeTimers();
  await init();
  expect(fetch).toHaveBeenCalledTimes(1);
  await vi.advanceTimersByTimeAsync(6 * 60 * 60 * 1000);
  expect(fetch).toHaveBeenCalledTimes(2);
});

test("turning the switch off hides the notice", async () => {
  await init();
  useConfig.setPath("config", full({ no_update_check: true }));
  await vi.waitFor(() => expect(useUpdate.getState().latest).toBeNull());
});
