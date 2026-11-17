import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { type Build, commands } from "@/bindings";
import { full } from "@/lib/config";
import { RequestProvider } from "@/lib/request-provider";
import { PlayButtonGroup } from "@/play/play-button-group";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";

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
    </RequestProvider>,
  );
}

const menu = () => screen.queryByRole("button", { name: "Pick a target" });

test("with no target, only a disabled Play shows", () => {
  show();
  expect(
    (screen.getByRole("button", { name: /^Play/ }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(menu()).toBeNull();
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
  built({
    status: { kind: "stale" },
    text: "FakeOne needs a rebuild: its hook patch changed since the last build",
    command: true,
  });
  show("FakeOne");
  fireEvent.click(screen.getByRole("button", { name: "Play FakeOne" }));
  expect(screen.getByText("Rebuild FakeOne first?")).toBeTruthy();
  expect(screen.getByRole("button", { name: "Rebuild" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "Cancel" })).toBeTruthy();
  expect(play).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "Play anyway" }));
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
