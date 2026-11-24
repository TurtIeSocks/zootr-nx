import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { commands } from "@/bindings";
import { full } from "@/lib/config";
import { useConfig } from "@/stores/config";
import { useUi } from "@/stores/ui";
import { checkNow, init, useUpdate } from "@/stores/update";

vi.mock("@/bindings", () => ({
  commands: {
    buildInfo: vi.fn(async () => ({
      version: "0.1.0",
      commit: "abc1234",
      release: false,
    })),
  },
}));
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
  vi.useRealTimers();
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

test("a newer release is kept, with this build's version", async () => {
  await init();
  expect(useUpdate.getState().latest?.version).toBe("0.2.0");
  expect(useUpdate.getState().current).toBe("0.1.0");
});

test("a source build keeps its commit and still hears of a release", async () => {
  await init();
  expect(useUpdate.getState()).toMatchObject({
    commit: "abc1234",
    release: false,
    latest: { version: "0.2.0" },
  });
});

test("a version it can't read leaves the version blank and offers nothing", async () => {
  vi.spyOn(console, "debug").mockImplementation(() => {});
  vi.mocked(commands.buildInfo).mockRejectedValueOnce(new Error("no Tauri"));
  await init();
  expect(useUpdate.getState().current).toBeNull();
  expect(useUpdate.getState().latest).toBeNull();
});

test("an older or equal release isn't", async () => {
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(release("v0.1.0")));
  await init();
  expect(useUpdate.getState().latest).toBeNull();
});

test("the switch off makes no request, and still reads the version", async () => {
  useConfig.setPath("config", full({ no_update_check: true }));
  await init();
  expect(fetch).not.toHaveBeenCalled();
  expect(useUpdate.getState().current).toBe("0.1.0");
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

test("a check still out when the switch turns off can't bring the notice back", async () => {
  useConfig.setPath("config", full({ no_update_check: true }));
  await init();
  let answer: (r: unknown) => void = () => {};
  vi.stubGlobal(
    "fetch",
    vi.fn(() => new Promise((r) => (answer = r))),
  );
  useConfig.setPath("config", full({}));
  useConfig.setPath("config", full({ no_update_check: true }));
  answer(release("v0.2.0"));
  await vi.waitFor(() => expect(fetch).toHaveBeenCalledTimes(1));
  await new Promise((r) => setTimeout(r));
  expect(useUpdate.getState().latest).toBeNull();
});

test("a check you ask for runs with checks off, and shows a skipped release", async () => {
  useConfig.setPath("config", full({ no_update_check: true }));
  await init();
  useUi.setPath("update.skipped", "0.2.0");
  expect(await checkNow()).toBe("newer");
  expect(useUpdate.getState().latest?.version).toBe("0.2.0");
  expect(useUi.getState().update.skipped).toBeNull();
});

test("a check you ask for says when it's up to date or can't reach GitHub", async () => {
  await init();
  vi.stubGlobal("fetch", vi.fn().mockResolvedValue(release("v0.1.0")));
  expect(await checkNow()).toBe("current");
  vi.spyOn(console, "debug").mockImplementation(() => {});
  vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("offline")));
  expect(await checkNow()).toBe("failed");
});
