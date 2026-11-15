import { openUrl } from "@tauri-apps/plugin-opener";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { useUi } from "@/stores/ui";
import { useUpdate } from "@/stores/update";
import { UpdateAlert } from "@/update/update-alert";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));

const RELEASE = {
  version: "0.2.0",
  url: "https://github.com/o/r/releases/tag/v0.2.0",
  notes:
    "## What's Changed\n\n* marks in [#12](https://github.com/o/r/pull/12)",
  published: "2026-11-14T12:00:00Z",
};

beforeEach(() => {
  useUpdate.setState({ latest: RELEASE, current: "0.1.0" });
});

afterEach(() => {
  cleanup();
  useUpdate.setState({ latest: null, current: null });
  useUi.setPath("update.skipped", null);
  vi.mocked(openUrl).mockClear();
});

const open = () =>
  fireEvent.click(screen.getByRole("button", { name: /Update available/ }));

test("nothing shows without a newer release", () => {
  useUpdate.setState({ latest: null });
  const { container } = render(<UpdateAlert />);
  expect(container.textContent).toBe("");
});

test("nothing shows for the skipped release", () => {
  useUi.setPath("update.skipped", "0.2.0");
  const { container } = render(<UpdateAlert />);
  expect(container.textContent).toBe("");
});

test("the popover shows the notes and opens the release page", () => {
  render(<UpdateAlert />);
  expect(screen.getByRole("button", { name: /v0\.2\.0/ })).toBeTruthy();
  open();
  expect(screen.getByText("zootr-nx v0.2.0")).toBeTruthy();
  expect(screen.getByText(/you have 0\.1\.0/)).toBeTruthy();
  expect(screen.getByRole("heading", { name: "What's Changed" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Open release page" }));
  expect(openUrl).toHaveBeenCalledWith(RELEASE.url);
});

test("a link in the notes opens in the browser", () => {
  render(<UpdateAlert />);
  open();
  fireEvent.click(screen.getByRole("link", { name: "#12" }));
  expect(openUrl).toHaveBeenCalledWith("https://github.com/o/r/pull/12");
});

test("Skip this version hides it", () => {
  render(<UpdateAlert />);
  open();
  fireEvent.click(screen.getByRole("button", { name: "Skip this version" }));
  expect(useUi.getState().update.skipped).toBe("0.2.0");
  expect(screen.queryByRole("button", { name: /Update available/ })).toBeNull();
});
