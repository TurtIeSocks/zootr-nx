import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { useUpdate } from "@/stores/update";
import { ChangelogButton } from "@/update/changelog";
import { PAGE } from "@/update/release";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

/** `n` releases counting down from `top`, as GitHub lists them. */
const releases = (top: number, n: number) =>
  Array.from({ length: n }, (_, i) => ({
    tag_name: `v0.${top - i}.0`,
    html_url: `https://github.com/o/r/releases/tag/v0.${top - i}.0`,
    body: `notes for 0.${top - i}.0`,
  }));

const page = (list: unknown[]) => ({ ok: true, json: async () => list });

beforeEach(() => {
  vi.stubEnv("VITE_UPDATE_REPO", "o/r");
  useUpdate.setState({ latest: null, current: "0.20.0" });
});

afterEach(() => {
  cleanup();
  useUpdate.setState({ latest: null, current: null });
  vi.unstubAllEnvs();
  vi.unstubAllGlobals();
});

test("the changelog fetches nothing until opened, then a page at a time", async () => {
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockResolvedValueOnce(page(releases(20, PAGE)))
      .mockResolvedValueOnce(page(releases(10, 2))),
  );
  render(<ChangelogButton />);
  expect(fetch).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Changelog" }));
  expect(await screen.findByText("notes for 0.20.0")).toBeTruthy();
  expect(screen.getByText("Current Version")).toBeTruthy();
  // Only the newest opens, and the rest render their notes once opened.
  expect(screen.queryByText("notes for 0.19.0")).toBeNull();
  const older = screen.getByRole("button", { name: /v0\.19\.0/ });
  expect(older.getAttribute("aria-expanded")).toBe("false");
  fireEvent.click(older);
  expect(screen.getByText("notes for 0.19.0")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Load older releases" }));
  expect(await screen.findByRole("button", { name: /v0\.9\.0/ })).toBeTruthy();
  expect(fetch).toHaveBeenCalledTimes(2);
  expect(
    screen.queryByRole("button", { name: "Load older releases" }),
  ).toBeNull();
});

test("the changelog says when it can't reach GitHub", async () => {
  vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("offline")));
  render(<ChangelogButton />);
  fireEvent.click(screen.getByRole("button", { name: "Changelog" }));
  expect(await screen.findByText(/Couldn't reach GitHub/)).toBeTruthy();
});

test("a failed fetch can be tried again", async () => {
  vi.stubGlobal(
    "fetch",
    vi
      .fn()
      .mockRejectedValueOnce(new TypeError("offline"))
      .mockResolvedValueOnce(page(releases(20, 2))),
  );
  render(<ChangelogButton />);
  fireEvent.click(screen.getByRole("button", { name: "Changelog" }));
  await screen.findByText(/Couldn't reach GitHub/);
  fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  expect(await screen.findByText("notes for 0.20.0")).toBeTruthy();
  expect(screen.queryByText(/Couldn't reach GitHub/)).toBeNull();
  expect(fetch).toHaveBeenCalledTimes(2);
});
