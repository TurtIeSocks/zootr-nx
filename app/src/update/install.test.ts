import { relaunch } from "@tauri-apps/plugin-process";
import {
  check,
  type DownloadEvent,
  type Update,
} from "@tauri-apps/plugin-updater";
import { afterEach, expect, test, vi } from "vitest";
import { install, installable, type Progress } from "@/update/install";

vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: vi.fn() }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn() }));

afterEach(() => vi.clearAllMocks());

/** An update that sends `events` while it downloads, then fails if `fail`. */
const fake = (version: string, events: DownloadEvent[] = [], fail = false) =>
  ({
    version,
    close: vi.fn(async () => {}),
    downloadAndInstall: vi.fn(async (on?: (e: DownloadEvent) => void) => {
      for (const e of events) on?.(e);
      if (fail) throw new Error("signature mismatch");
    }),
  }) as unknown as Update;

test("installable gives the update whose version the notice names", async () => {
  const update = fake("0.5.0");
  vi.mocked(check).mockResolvedValue(update);
  expect(await installable("0.5.0")).toBe(update);
  expect(await installable("0.6.0")).toBeNull();
  expect(update.close).toHaveBeenCalledTimes(1);
});

test("installable is null without an update or when the check fails", async () => {
  vi.spyOn(console, "debug").mockImplementation(() => {});
  vi.mocked(check).mockResolvedValue(null);
  expect(await installable("0.5.0")).toBeNull();
  vi.mocked(check).mockRejectedValue(new Error("no latest.json"));
  expect(await installable("0.5.0")).toBeNull();
});

test("install reports progress up to the total, then restarts", async () => {
  const seen: Progress[] = [];
  await install(
    fake("0.5.0", [
      { event: "Started", data: { contentLength: 10 } },
      { event: "Progress", data: { chunkLength: 4 } },
      { event: "Progress", data: { chunkLength: 6 } },
      { event: "Finished" },
    ]),
    (p) => seen.push(p),
  );
  expect(seen).toEqual([
    { done: 0, total: 10 },
    { done: 4, total: 10 },
    { done: 10, total: 10 },
  ]);
  expect(relaunch).toHaveBeenCalledOnce();
});

test("a failed install throws and doesn't restart", async () => {
  await expect(install(fake("0.5.0", [], true), () => {})).rejects.toThrow(
    "signature mismatch",
  );
  expect(relaunch).not.toHaveBeenCalled();
});
