import {
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { DevWindow } from "@/dev/dev-window";
import { RECENT_PLACES } from "@/dev/requests";
import { useUi } from "@/stores/ui";
import { devActors, devTeleport, devWarp, err, game } from "@/test/dev-game";
import { buttonsIn, fakeGame, renderReady } from "@/test/dev-window";

vi.mock("@/bindings", async () => (await import("@/test/dev-game")).bindings);
vi.mock(
  "@tauri-apps/api/window",
  async () => (await import("@/test/dev-game")).fakeWindow,
);

fakeGame();

const MARKET = "FakeHill_FrontOfMarket";
const GATE = "FakeKakariko_FrontOfGate";

/** The warp button of the place `spot` in the list labelled `list`. */
const place = (list: string, spot: string) =>
  within(screen.getByRole("list", { name: list })).getByRole("button", {
    name: new RegExp(`^${spot}`),
  });

test("lists the places for Link's age by area", async () => {
  await renderReady();
  expect(buttonsIn("FakeHill").map((b) => b.textContent)).toEqual([MARKET, ""]);
  expect(screen.getByRole("list", { name: "FakeKakariko" })).toBeTruthy();
});

test("a teleport says what happened in the activity line, and a refusal shows as one", async () => {
  await renderReady();
  fireEvent.click(place("FakeHill", MARKET));
  expect(devTeleport).toHaveBeenCalledWith(`FakeHill_Child/${MARKET}`);
  expect((await screen.findByRole("status")).textContent).toBe("sent");

  devTeleport.mockImplementationOnce(() => err("an event is playing"));
  fireEvent.click(place("FakeHill", MARKET));
  expect((await screen.findByRole("alert")).textContent).toBe(
    `Couldn't warp to ${MARKET}: an event is playing`,
  );
});

test("the search narrows the places", async () => {
  await renderReady();
  fireEvent.change(screen.getByRole("textbox", { name: "Search places" }), {
    target: { value: "gate" },
  });
  expect(screen.queryByRole("list", { name: "FakeHill" })).toBeNull();
  expect(screen.getByRole("list", { name: "FakeKakariko" })).toBeTruthy();
});

test("a star toggles, persists, and lists the place under Starred", async () => {
  await renderReady();
  expect(screen.queryByRole("list", { name: "Starred" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: `Star ${GATE}` }));
  expect(useUi.getPath("dev.starred")).toEqual([`FakeKakariko_Child/${GATE}`]);
  const starred = screen.getByRole("list", { name: "Starred" });
  expect(within(starred).getByText("FakeKakariko")).toBeTruthy();
  fireEvent.click(place("Starred", GATE));
  expect(devTeleport).toHaveBeenCalledWith(`FakeKakariko_Child/${GATE}`);

  cleanup();
  await renderReady();
  const unstar = screen.getAllByRole("button", { name: `Unstar ${GATE}` });
  expect(unstar[0]?.getAttribute("aria-pressed")).toBe("true");
  fireEvent.click(unstar[0] as HTMLElement);
  expect(useUi.getPath("dev.starred")).toEqual([]);
  expect(screen.queryByRole("list", { name: "Starred" })).toBeNull();
});

test("a warp that works heads Recent, with no repeats, keeping more than it shows", async () => {
  useUi.setPath(
    "dev.recent.places",
    ["1", "2", "3", "4", "5"].map((n) => `FakeHill_Child/Fake_${n}`),
  );
  await renderReady();
  fireEvent.click(place("FakeKakariko", GATE));
  await vi.waitFor(() =>
    expect(useUi.getPath("dev.recent.places")[0]).toBe(
      `FakeKakariko_Child/${GATE}`,
    ),
  );
  expect(useUi.getPath("dev.recent.places")).toHaveLength(6);
  const shown = screen.getByRole("list", { name: "Recent" });
  expect(within(shown).getAllByRole("listitem")).toHaveLength(RECENT_PLACES);
  fireEvent.click(place("FakeHill", MARKET));
  await vi.waitFor(() =>
    expect(useUi.getPath("dev.recent.places").slice(0, 2)).toEqual([
      `FakeHill_Child/${MARKET}`,
      `FakeKakariko_Child/${GATE}`,
    ]),
  );
  fireEvent.click(place("Recent", GATE));
  await vi.waitFor(() =>
    expect(useUi.getPath("dev.recent.places").slice(0, 2)).toEqual([
      `FakeKakariko_Child/${GATE}`,
      `FakeHill_Child/${MARKET}`,
    ]),
  );
  expect(useUi.getPath("dev.recent.places")).toHaveLength(7);

  devTeleport.mockImplementationOnce(() => err("an event is playing"));
  const before = useUi.getPath("dev.recent.places");
  fireEvent.click(place("FakeHill", MARKET));
  await screen.findByRole("alert");
  expect(useUi.getPath("dev.recent.places")).toEqual(before);
});

test("Actors in Map lists the map's actors, and search narrows them", async () => {
  await renderReady();
  fireEvent.click(screen.getByRole("button", { name: "Actors in Map" }));
  const list = await screen.findByRole("list", {
    name: "Actors in FakeHill_Child",
  });
  expect(devActors).toHaveBeenCalledWith("FakeHill_Child");
  expect(await within(list).findByText("Field_000")).toBeTruthy();
  expect(within(list).getByText("#2")).toBeTruthy();
  expect(within(list).getAllByRole("listitem")).toHaveLength(3);
  const search = screen.getByRole("textbox", { name: "Search actors" });
  fireEvent.change(search, { target: { value: "npc" } });
  expect(within(list).getAllByRole("listitem")).toHaveLength(1);
  fireEvent.change(search, { target: { value: "field_0" } });
  expect(within(list).getAllByRole("listitem")).toHaveLength(1);
  // Its tag counts over the whole map, so the search keeps its label.
  expect(within(list).getByText("Field_000")).toBeTruthy();
});

test("a click on an actor warps to the picked side", async () => {
  await renderReady();
  fireEvent.click(screen.getByRole("button", { name: "Actors in Map" }));
  const npc = await screen.findByRole("button", { name: "Npc_A" });
  fireEvent.click(screen.getByRole("button", { name: "Back" }));
  fireEvent.click(npc);
  expect(devWarp).toHaveBeenCalledWith("1", "back");
  expect((await screen.findByRole("status")).textContent).toBe(
    "warped beside 1",
  );
});

test("without a map Actors in map waits for one", async () => {
  game.state = undefined;
  useUi.setPath("dev.teleport.tab", "actors");
  render(<DevWindow />);
  fireEvent.click(await screen.findByRole("tab", { name: "Warp" }));
  expect(await screen.findByText("Waiting for the map")).toBeTruthy();
  expect(devActors).not.toHaveBeenCalled();
});

test("the picked tabs and searches are kept in the view", async () => {
  await renderReady();
  fireEvent.change(screen.getByRole("textbox", { name: "Search places" }), {
    target: { value: "gate" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Actors in Map" }));
  fireEvent.click(screen.getByRole("tab", { name: "Tests" }));
  cleanup();
  render(<DevWindow />);
  const tests = await screen.findByRole("tab", { name: "Tests" });
  expect(tests.getAttribute("aria-selected")).toBe("true");
  fireEvent.click(screen.getByRole("tab", { name: "Warp" }));
  expect(await screen.findByRole("list", { name: /^Actors in/ })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Places" }));
  expect(
    (screen.getByRole("textbox", { name: "Search places" }) as HTMLInputElement)
      .value,
  ).toBe("gate");
});

test("a starred place leaves Recent, which still shows five, and comes back once unstarred", async () => {
  const recent = ["1", "2", "3", "4", "5", "6"].map(
    (n) => `FakeHill_Child/Fake_${n}`,
  );
  useUi.setPath("dev.recent.places", recent);
  useUi.setPath("dev.starred", ["FakeHill_Child/Fake_1"]);
  await renderReady();
  const shown = () =>
    within(screen.getByRole("list", { name: "Recent" }))
      .getAllByRole("listitem")
      .map((li) => li.textContent);
  expect(shown()).toHaveLength(RECENT_PLACES);
  expect(shown()[0]).toMatch(/^Fake_2/);
  fireEvent.click(
    screen.getAllByRole("button", { name: "Unstar Fake_1" })[0] as HTMLElement,
  );
  expect(shown()[0]).toMatch(/^Fake_1/);
  expect(useUi.getPath("dev.recent.places")).toEqual(recent);
});

/** Warp's actor list, once read. */
async function warpActors() {
  await renderReady();
  fireEvent.click(screen.getByRole("button", { name: "Actors in Map" }));
  const list = await screen.findByRole("list", {
    name: "Actors in FakeHill_Child",
  });
  await within(list).findByText("Field_000");
  return list;
}

const rowNames = (list: HTMLElement) =>
  within(list)
    .getAllByRole("listitem")
    .map((li) => li.querySelector("span")?.textContent);

test("a map actor's star saves its placement and lists it under Starred", async () => {
  await warpActors();
  expect(screen.queryByRole("list", { name: "Starred actors" })).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Star Npc_A" }));
  expect(useUi.getPath("dev.stars.actors")).toEqual(["1"]);
  const starred = screen.getByRole("list", { name: "Starred actors" });
  expect(rowNames(starred)).toEqual(["Npc_A"]);
  fireEvent.click(
    screen.getAllByRole("button", { name: "Unstar Npc_A" })[0] as HTMLElement,
  );
  expect(useUi.getPath("dev.stars.actors")).toEqual([]);
});

test("a star kept for another map's placement shows nothing here and stays saved", async () => {
  useUi.setPath("dev.stars.actors", ["999"]);
  await warpActors();
  expect(screen.queryByRole("list", { name: "Starred actors" })).toBeNull();
  expect(useUi.getPath("dev.stars.actors")).toEqual(["999"]);
});

test("Nearest sorts Warp's actors by placed distance, leaving the other lists' order", async () => {
  const list = await warpActors();
  expect(rowNames(list)).toEqual(["Npc_A", "Obj_A", "Obj_A"]);
  fireEvent.click(screen.getByRole("button", { name: "Nearest" }));
  await vi.waitFor(() => expect(within(list).getByText("1.0 m")).toBeTruthy());
  expect(
    within(list)
      .getAllByRole("listitem")
      .map((li) =>
        [...li.querySelectorAll("span")].map((span) => span.textContent),
      ),
  ).toEqual([
    ["Obj_A", "#2", "1.0 m"],
    ["Obj_A", "Field_000", "5.0 m"],
    ["Npc_A", "10.0 m"],
  ]);
  expect(useUi.getPath("dev.sort")).toEqual({
    warp: "near",
    all: "az",
    nearby: "near",
  });
});
