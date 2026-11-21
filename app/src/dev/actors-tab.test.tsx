import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import type { DevNear, DevState } from "@/bindings";
import { Activity } from "@/dev/activity";
import { ActorsTab, LiveReads } from "@/dev/actors-tab";
import { FOLLOW_MS, RETRY_MS } from "@/dev/use-live-actors";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

type Result<T> = { status: "ok"; data: T };
const ok = <T,>(data: T): Promise<Result<T>> =>
  Promise.resolve({ status: "ok", data });

const near = (
  actor: string,
  hash: string,
  distance: number,
  hidden: DevNear["hidden"] = "shown",
  label: string | null = null,
): DevNear => ({ actor, hash, distance, hidden, label });

/** What the game holds: its actors, and which placements are live. */
const world = {
  list: [] as DevNear[],
  live: new Set<string>(),
};
const devNearby = vi.fn(() => ok(world.list));
const devHide = vi.fn((hashes: string[], _on: boolean) =>
  ok(hashes.filter((h) => world.live.has(h))),
);

const devWarp = vi.fn((hash: string, side: string) =>
  ok(`warped beside ${hash} ${side}`),
);

vi.mock("@/bindings", () => ({
  commands: {
    devNearby: () => devNearby(),
    devHide: (h: string[], on: boolean) => devHide(h, on),
    devWarp: (h: string, s: string) => devWarp(h, s),
    devActors: () => ok([{ actor: "Obj_FakeRock", hash: "1", label: null }]),
  },
}));

/** The Actors tab as the window holds it: its reads, and the activity
 * line its results land in. */
const liveActors = (
  <>
    <LiveReads />
    <ActorsTab />
    <Activity />
  </>
);

const state = (map: string, loading = false): DevState => ({
  map,
  child: true,
  loading,
  event: false,
  switches: { health: false, magic: false, resources: false },
});

/** Link stands on `map`, the game answering. */
const onMap = (map: string, loading = false) =>
  act(() => {
    useDev.setPath("game", { state: state(map, loading) });
  });

beforeEach(() => {
  world.list = [
    near("Obj_FakeRock", "1", 1.5),
    near("Land_FakeIvy", "2", 3.25, "shown", "FakeIvy_000"),
    near("Land_FakeIvy", "3", 4),
    near("Npc_FakeWaiting", "4", 6, "game"),
  ];
  world.live = new Set(["1", "2", "3", "4"]);
  useDev.setPath("live", {
    read: { status: "loading" },
    hidden: [],
    follow: false,
    query: "",
  });
  useDev.setPath("game", { state: state("FakeHill_Child") });
  useDev.setPath("activity", { log: [], running: null });
  useUi.setPath("dev.actors.tab", "nearby");
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.useRealTimers();
});

const list = () => screen.findByRole("list", { name: /^Live actors in/ });
/** A row's hide toggle: the button of its row that's pressed or not. */
const row = (name: RegExp) => {
  const found = within(screen.getByRole("list", { name: /^Live actors in/ }))
    .getAllByRole("button", { name })
    .find((b) => b.hasAttribute("aria-pressed"));
  if (!found) throw new Error(`no row for ${name}`);
  return found;
};

test("lists the live actors nearest first, the game's hidden ones untoggled", async () => {
  render(liveActors);
  const rows = within(await list()).findAllByRole("listitem");
  const text = (await rows).map((r) => r.textContent);
  expect(text).toEqual([
    "Obj_FakeRock1.5 m",
    "Land_FakeIvyFakeIvy_0003.3 m",
    "Land_FakeIvy#24.0 m",
    "Npc_FakeWaitinghidden by game",
  ]);
  expect(screen.queryByRole("button", { name: /Npc_FakeWaiting/ })).toBeNull();
  expect(screen.queryByRole("list", { name: "Hidden actors" })).toBeNull();
});

test("a click hides the actor, listing it in the strip, and again shows it", async () => {
  render(liveActors);
  await within(await list()).findByText("Obj_FakeRock");
  fireEvent.click(row(/Obj_FakeRock/));
  await vi.waitFor(() =>
    expect(row(/Obj_FakeRock/).getAttribute("aria-pressed")).toBe("true"),
  );
  expect(devHide).toHaveBeenCalledWith(["1"], true);
  expect(screen.getByRole("status").textContent).toBe(
    "Hid Obj_FakeRock in FakeHill_Child",
  );
  const strip = screen.getByRole("list", { name: "Hidden actors" });
  expect(within(strip).getByText("Obj_FakeRock")).toBeTruthy();
  expect(useDev.getPath("live.hidden")).toEqual([
    { hash: "1", actor: "Obj_FakeRock", label: null, map: "FakeHill_Child" },
  ]);

  fireEvent.click(row(/Obj_FakeRock/));
  await vi.waitFor(() =>
    expect(row(/Obj_FakeRock/).getAttribute("aria-pressed")).toBe("false"),
  );
  expect(devHide).toHaveBeenLastCalledWith(["1"], false);
  expect(screen.queryByRole("list", { name: "Hidden actors" })).toBeNull();
});

test("an actor gone since the read says so and stays off the strip", async () => {
  render(liveActors);
  await within(await list()).findByText("Obj_FakeRock");
  world.live.delete("1");
  fireEvent.click(row(/Obj_FakeRock/));
  expect((await screen.findByRole("alert")).textContent).toBe(
    "Couldn't hide Obj_FakeRock: Obj_FakeRock isn't live anymore. Refresh the list",
  );
  expect(useDev.getPath("live.hidden")).toEqual([]);
});

test("a chip's × shows its actor, and Show all shows the rest", async () => {
  render(liveActors);
  await within(await list()).findByText("Obj_FakeRock");
  for (const name of [/Obj_FakeRock/, /FakeIvy_000/, /#2/]) {
    fireEvent.click(row(name));
    await vi.waitFor(() =>
      expect(row(name).getAttribute("aria-pressed")).toBe("true"),
    );
  }
  const strip = screen.getByRole("list", { name: "Hidden actors" });
  expect(within(strip).getAllByRole("listitem")).toHaveLength(3);

  fireEvent.click(
    within(strip).getByRole("button", { name: "Show Obj_FakeRock" }),
  );
  await vi.waitFor(() =>
    expect(within(strip).getAllByRole("listitem")).toHaveLength(2),
  );
  expect(devHide).toHaveBeenLastCalledWith(["1"], false);

  fireEvent.click(screen.getByRole("button", { name: "Show All" }));
  await vi.waitFor(() =>
    expect(screen.queryByRole("list", { name: "Hidden actors" })).toBeNull(),
  );
  expect(devHide).toHaveBeenLastCalledWith(["2", "3"], false);
  expect(row(/#2/).getAttribute("aria-pressed")).toBe("false");
});

test("the search narrows the rows, keeping their order and tags", async () => {
  render(liveActors);
  await within(await list()).findByText("Obj_FakeRock");
  fireEvent.change(
    screen.getByRole("textbox", { name: "Search live actors" }),
    {
      target: { value: "ivy" },
    },
  );
  const rows = within(await list()).getAllByRole("listitem");
  expect(rows.map((r) => r.textContent)).toEqual([
    "Land_FakeIvyFakeIvy_0003.3 m",
    "Land_FakeIvy#24.0 m",
  ]);
});

test("after a map load it hides the map's listed actors again, retrying ones not made yet", async () => {
  vi.useFakeTimers();
  useDev.setPath("live.hidden", [
    { hash: "1", actor: "Obj_FakeRock", label: null, map: "FakeHill_Child" },
    { hash: "3", actor: "Land_FakeIvy", label: null, map: "FakeHill_Child" },
    { hash: "9", actor: "Obj_FakeElse", label: null, map: "FakeLake_Child" },
  ]);
  world.live = new Set(["1"]);
  onMap("FakeHill_Child", true);
  render(liveActors);
  await vi.advanceTimersByTimeAsync(0);
  expect(devHide).not.toHaveBeenCalled();

  onMap("FakeHill_Child");
  await vi.advanceTimersByTimeAsync(0);
  // The read hides what it lists, the re-hide tries what's left.
  expect(devHide.mock.calls).toEqual([
    [["1", "3"], true],
    [["3"], true],
  ]);

  world.live.add("3");
  await vi.advanceTimersByTimeAsync(RETRY_MS);
  expect(devHide).toHaveBeenLastCalledWith(["3"], true);
  await vi.advanceTimersByTimeAsync(RETRY_MS * 5);
  expect(devHide).toHaveBeenCalledTimes(3);
  expect(row(/Obj_FakeRock/).getAttribute("aria-pressed")).toBe("true");
  expect(row(/#2/).getAttribute("aria-pressed")).toBe("true");
});

test("a read hides again a listed actor made again with no map load", async () => {
  render(liveActors);
  await within(await list()).findByText("Obj_FakeRock");
  fireEvent.click(row(/Obj_FakeRock/));
  await vi.waitFor(() =>
    expect(row(/Obj_FakeRock/).getAttribute("aria-pressed")).toBe("true"),
  );
  // Link walks out of the room and back in: the rock is made again, shown.
  devHide.mockClear();
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  await vi.waitFor(() => expect(devHide).toHaveBeenCalledWith(["1"], true));
  expect(row(/Obj_FakeRock/).getAttribute("aria-pressed")).toBe("true");
  expect(row(/#2/).getAttribute("aria-pressed")).toBe("false");
});

test("a re-hide gives up after its retries and stops once Link leaves", async () => {
  vi.useFakeTimers();
  useDev.setPath("live.hidden", [
    { hash: "7", actor: "Obj_FakeLate", label: null, map: "FakeHill_Child" },
  ]);
  render(liveActors);
  await vi.advanceTimersByTimeAsync(RETRY_MS * 10);
  expect(devHide).toHaveBeenCalledTimes(5);

  devHide.mockClear();
  onMap("FakeLake_Child");
  await vi.advanceTimersByTimeAsync(0);
  onMap("FakeHill_Child");
  await vi.advanceTimersByTimeAsync(0);
  expect(devHide).toHaveBeenCalledTimes(1);
  onMap("FakeLake_Child");
  await vi.advanceTimersByTimeAsync(RETRY_MS * 10);
  expect(devHide).toHaveBeenCalledTimes(1);
  // The chip stays, dimmed, for the next load of its map.
  const strip = screen.getByRole("list", { name: "Hidden actors" });
  expect(within(strip).getByText("Obj_FakeLate")).toBeTruthy();
});

test("Follow reads again every few seconds, only while on", async () => {
  vi.useFakeTimers();
  render(liveActors);
  await vi.advanceTimersByTimeAsync(FOLLOW_MS * 3);
  expect(devNearby).toHaveBeenCalledTimes(1);

  fireEvent.click(screen.getByRole("switch", { name: "Follow" }));
  await vi.advanceTimersByTimeAsync(FOLLOW_MS * 3);
  expect(devNearby).toHaveBeenCalledTimes(4);

  fireEvent.click(screen.getByRole("switch", { name: "Follow" }));
  await vi.advanceTimersByTimeAsync(FOLLOW_MS * 3);
  expect(devNearby).toHaveBeenCalledTimes(4);
});

test("Refresh reads again in the new order", async () => {
  render(liveActors);
  await within(await list()).findByText("Obj_FakeRock");
  world.list = [near("Npc_FakeNew", "5", 0.5), ...world.list];
  fireEvent.click(screen.getByRole("button", { name: "Refresh" }));
  const first = await within(await list()).findByText("Npc_FakeNew");
  expect(first).toBeTruthy();
  expect(devNearby).toHaveBeenCalledTimes(2);
});

test("a row's Warp beside warps to it on the side picked in Warp", async () => {
  render(liveActors);
  await within(await list()).findByText("Obj_FakeRock");
  fireEvent.click(
    screen.getByRole("button", { name: "Warp beside Land_FakeIvy #2" }),
  );
  expect(devWarp).toHaveBeenCalledWith("3", "front");
  expect((await screen.findByRole("status")).textContent).toBe(
    "warped beside 3 front",
  );
});

test("a hidden row is struck through, and the list says when it was read", async () => {
  render(liveActors);
  await within(await list()).findByText("Obj_FakeRock");
  expect(screen.getByText(/^read/).textContent).toBe("read now");
  fireEvent.click(row(/Obj_FakeRock/));
  await vi.waitFor(() =>
    expect(
      within(screen.getByRole("list", { name: /^Live actors in/ })).getByText(
        "Obj_FakeRock",
      ).className,
    ).toContain("line-through"),
  );
});

test("All in Map lists the map's placements, each a warp away", async () => {
  render(liveActors);
  fireEvent.click(screen.getByRole("button", { name: "All in Map" }));
  const all = await screen.findByRole("list", {
    name: "Actors in FakeHill_Child",
  });
  expect(
    await within(all).findByRole("button", { name: "Obj_FakeRock" }),
  ).toBeTruthy();
});
