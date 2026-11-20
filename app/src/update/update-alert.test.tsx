import { openUrl } from "@tauri-apps/plugin-opener";
import type { Update } from "@tauri-apps/plugin-updater";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";
import { useUpdate } from "@/stores/update";
import { install, installable } from "@/update/install";
import { UpdateAlert } from "@/update/update-alert";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn() }));
vi.mock("@/update/install", () => ({ installable: vi.fn(), install: vi.fn() }));

const UPDATE = { version: "0.2.0" } as Update;

const RELEASE = {
  version: "0.2.0",
  url: "https://github.com/o/r/releases/tag/v0.2.0",
  changelog: "https://github.com/o/r/blob/main/CHANGELOG.md",
  notes:
    "## What's Changed\n\n* marks in [#12](https://github.com/o/r/pull/12)",
  published: "2026-11-14T12:00:00Z",
};

beforeEach(() => {
  useUpdate.setState({ latest: RELEASE, current: "0.1.0" });
  vi.mocked(installable).mockResolvedValue(null);
});

afterEach(() => {
  cleanup();
  useUpdate.setState({ latest: null, current: null });
  useUi.setPath("update.skipped", null);
  useSeeds.setPath("running", false);
  vi.clearAllMocks();
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
  expect(screen.getByText("Published Nov 14, 2026")).toBeTruthy();
  expect(screen.getByRole("heading", { name: "What's Changed" })).toBeTruthy();
  fireEvent.click(screen.getByRole("link", { name: "Release page" }));
  expect(openUrl).toHaveBeenCalledWith(RELEASE.url);
  fireEvent.click(screen.getByRole("link", { name: "Changelog" }));
  expect(openUrl).toHaveBeenCalledWith(RELEASE.changelog);
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

/** Opens the popover and lets the updater check settle. */
const openSettled = async () => {
  render(<UpdateAlert />);
  open();
  await act(async () => {});
};

const installButton = () =>
  screen.queryByRole("button", { name: "Install & restart" });

test("no install without an update the updater can install", async () => {
  await openSettled();
  expect(installable).toHaveBeenCalledWith("0.2.0");
  expect(installButton()).toBeNull();
  expect(screen.getByRole("link", { name: "Release page" })).toBeTruthy();
});

test("Install & restart installs the update", async () => {
  vi.mocked(installable).mockResolvedValue(UPDATE);
  vi.mocked(install).mockReturnValue(new Promise(() => {}));
  await openSettled();
  fireEvent.click(installButton() as HTMLElement);
  expect(install).toHaveBeenCalledWith(UPDATE, expect.any(Function));
  expect(screen.getByRole("button", { name: "Installing…" })).toHaveProperty(
    "disabled",
    true,
  );
  expect(screen.getByRole("progressbar")).toBeTruthy();
});

test("a running game holds the install", async () => {
  vi.mocked(installable).mockResolvedValue(UPDATE);
  useSeeds.setPath("running", true);
  await openSettled();
  const button = installButton() as HTMLElement;
  expect(button.getAttribute("aria-disabled")).toBe("true");
  await act(async () => fireEvent.click(button));
  expect(install).not.toHaveBeenCalled();
});

test("a failed install says so and keeps the release page", async () => {
  vi.mocked(installable).mockResolvedValue(UPDATE);
  vi.mocked(install).mockRejectedValue("signature mismatch");
  await openSettled();
  await act(async () => fireEvent.click(installButton() as HTMLElement));
  expect(screen.getByText(/The update didn't install/)).toBeTruthy();
  expect(screen.queryByText("signature mismatch")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Details" }));
  expect(screen.getByText("signature mismatch")).toBeTruthy();
  expect(installButton()).toHaveProperty("disabled", false);
  expect(screen.getByRole("link", { name: "Release page" })).toBeTruthy();
});
