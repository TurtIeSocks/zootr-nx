import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { type Build, commands } from "@/bindings";
import { RequestError } from "@/components/request-error";
import { full, NEED_TARGET } from "@/lib/config";
import { RequestProvider } from "@/lib/request-provider";
import { PlayButtonGroup } from "@/play/play-button-group";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

// The step log's channel, which needs the app's window.
vi.mock("@tauri-apps/api/core", () => ({
  Channel: class {},
  invoke: vi.fn(),
}));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useSeeds.setPath("status", { game: true, targets: [] });
});

/** Renders the group with targets named `names`. */
function show(...names: string[]) {
  useConfig.setPath(
    "config",
    full({
      targets: names.map((name) => ({ name, app: "/a", mod_dir: "/m" })),
    }),
  );
  render(
    <RequestProvider>
      <PlayButtonGroup />
      <RequestError />
    </RequestProvider>,
  );
}

const menu = () => screen.queryByRole("button", { name: "Pick a target" });

test("with no target, only a Play that says why shows", async () => {
  const play = vi.spyOn(commands, "play");
  show();
  const button = screen.getByRole("button", { name: "Play" });
  expect(button.getAttribute("aria-disabled")).toBe("true");
  expect(menu()).toBeNull();
  // Focus shows its tip, which says what's missing.
  fireEvent.focus(button);
  expect((await screen.findByRole("tooltip")).textContent).toBe(NEED_TARGET);
  // A click opens where a target is added, and plays nothing.
  useUi.setPath("tab", "Play");
  fireEvent.click(button);
  expect(play).not.toHaveBeenCalled();
  expect(useUi.getState().tab).toBe("Settings");
  expect(useUi.getState().section).toBe("targets");
});

test("with one target, only Play shows", () => {
  show("FakeOne");
  expect(screen.getByRole("button", { name: "Play FakeOne" })).toBeTruthy();
  expect(menu()).toBeNull();
});

test("with two targets, the menu picks between them", () => {
  show("FakeOne", "FakeTwo");
  expect(screen.getByRole("button", { name: "Play FakeOne" })).toBeTruthy();
  expect(menu()).toBeTruthy();
});

/** A build whose hook patch changed, that the app can rebuild. */
const STALE: Build = {
  status: { kind: "stale" },
  text: "FakeOne needs a rebuild: its hook patch changed since the last build",
  command: true,
};

/** The status of FakeOne's game, built with `build`. */
function built(build: Build) {
  useSeeds.setPath("status", {
    game: true,
    targets: [
      { name: "FakeOne", app: true, mod_dir: true, running: false, build },
    ],
  });
}

test("Play on a stale target asks first, and Play anyway starts it", () => {
  const play = vi
    .spyOn(commands, "play")
    .mockResolvedValue({ status: "ok", data: null });
  built(STALE);
  show("FakeOne");
  fireEvent.click(screen.getByRole("button", { name: "Play FakeOne" }));
  expect(screen.getByText("Rebuild FakeOne first?")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Rebuild and Play" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();
  expect(play).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Play Anyway" }));
  expect(play).toHaveBeenCalledWith("FakeOne");
  expect(screen.queryByText("Rebuild FakeOne first?")).toBeNull();
});

test("Play on a current build starts at once", () => {
  const play = vi
    .spyOn(commands, "play")
    .mockResolvedValue({ status: "ok", data: null });
  built({ status: { kind: "current" }, text: null, command: true });
  show("FakeOne");
  fireEvent.click(screen.getByRole("button", { name: "Play FakeOne" }));
  expect(play).toHaveBeenCalledWith("FakeOne");
  expect(screen.queryByText("Rebuild FakeOne first?")).toBeNull();
});

/** FakeOne stale with a command, Play pressed, then Rebuild and play,
 * the rebuild answering `answer`. Returns the play spy. */
function rebuildAndPlay(
  answer: { status: "ok"; data: null } | { status: "error"; error: string },
) {
  vi.spyOn(commands, "status").mockResolvedValue({
    status: "ok",
    data: { game: true, targets: [] },
  });
  vi.spyOn(commands, "history").mockResolvedValue({
    status: "ok",
    data: { installed: null, entries: [] },
  });
  vi.spyOn(commands, "rebuild").mockResolvedValue(answer);
  const play = vi
    .spyOn(commands, "play")
    .mockResolvedValue({ status: "ok", data: null });
  built(STALE);
  show("FakeOne");
  fireEvent.click(screen.getByRole("button", { name: "Play FakeOne" }));
  fireEvent.click(screen.getByRole("button", { name: "Rebuild and Play" }));
  return play;
}

test("Rebuild and Play starts the game once the build is current", async () => {
  const play = rebuildAndPlay({ status: "ok", data: null });
  await waitFor(() => expect(play).toHaveBeenCalledWith("FakeOne"));
  expect(commands.rebuild).toHaveBeenCalledWith("FakeOne", expect.anything());
});

test("a rebuild that leaves the build stale shows why and doesn't play", async () => {
  const still =
    "FakeOne's rebuild command finished, but the build still doesn't have the installed hook patch";
  const play = rebuildAndPlay({ status: "error", error: still });
  expect(await screen.findByText(still)).toBeTruthy();
  expect(play).not.toHaveBeenCalled();
});

/** FakeOne and FakeTwo, with FakeOne's game running. */
function oneRunning() {
  useSeeds.setPath("status", {
    game: true,
    targets: ["FakeOne", "FakeTwo"].map((name) => ({
      name,
      app: true,
      mod_dir: true,
      running: name === "FakeOne",
    })),
  });
}

test("Play says why it can't start a target already running", () => {
  const play = vi.spyOn(commands, "play");
  oneRunning();
  show("FakeOne", "FakeTwo");
  const button = screen.getByRole("button", { name: "Play FakeOne" });
  expect(button.getAttribute("aria-disabled")).toBe("true");
  fireEvent.click(button);
  expect(play).not.toHaveBeenCalled();
});

test("another target still plays while one runs", () => {
  oneRunning();
  show("FakeOne", "FakeTwo");
  act(() => useConfig.setPath("config.picked", "FakeTwo"));
  const button = screen.getByRole("button", { name: "Play FakeTwo" });
  expect(button.getAttribute("aria-disabled")).toBeNull();
});
