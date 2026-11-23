import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { Cell, ItemsState, TrackerOptions } from "@/bindings";
import { ItemsView } from "@/items/items-view";
import { TrackerWindow } from "@/items/tracker-window";
import { init, stop, useItems } from "@/stores/items";
import { useUi } from "@/stores/ui";

const cell = (label: string, patch: Partial<Cell> = {}): Cell => ({
  section: "Inventory",
  label,
  icon: `Item_${label}`,
  glyph: "unknown",
  tier: 1,
  tiers: 1,
  count: null,
  row: null,
  hidden: false,
  ...patch,
});

const backend = {
  listeners: [] as ((e: { payload: ItemsState }) => void)[],
  state: {
    cells: [
      cell("Bombs", { count: 20 }),
      cell("Hammer", { tier: 0 }),
      cell("Bomb Bag", { section: "Upgrades", tier: 2, tiers: 3 }),
      cell("Sun's Song", { section: "Songs" }),
    ],
    options: {
      size: "Medium",
      background: "#00ff00",
      sections: [
        { section: "Inventory", column: 1, width: 2, shown: true },
        { section: "Upgrades", column: 1, width: 1, shown: true },
        { section: "Songs", column: 1, width: 1, shown: false },
      ],
    },
    game_art: true,
  } as ItemsState,
  /** Resolves the icon build. Tests that don't care get it built already. */
  icons: Promise.resolve({ status: "ok", data: null }) as Promise<unknown>,
  /** The icon build command, for tests that count its calls. */
  build: (): Promise<unknown> => backend.icons,
  /** Why popping out fails, if it does. */
  popOutFail: undefined as string | undefined,
  send(payload: ItemsState) {
    act(() => {
      for (const l of backend.listeners) l({ payload });
    });
  },
};

const itemsOptions = vi.fn((_o: TrackerOptions) =>
  Promise.resolve({ status: "ok", data: null }),
);

vi.mock("@/bindings", () => ({
  TRACKER_COLUMNS: 3,
  events: {
    itemsEvent: {
      listen: (cb: (e: { payload: ItemsState }) => void) => {
        backend.listeners.push(cb);
        return Promise.resolve(() => {});
      },
    },
  },
  commands: {
    itemsState: () => Promise.resolve({ status: "ok", data: backend.state }),
    itemsOptions: (o: TrackerOptions) => itemsOptions(o),
    itemsPopOut: () =>
      Promise.resolve(
        backend.popOutFail
          ? { status: "error", error: backend.popOutFail }
          : { status: "ok", data: null },
      ),
    itemsIcons: () => backend.build(),
  },
}));

const setSize = vi.fn();
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setSize }),
  LogicalSize: class {},
}));

globalThis.ResizeObserver = class {
  observe() {}
  disconnect() {}
} as unknown as typeof ResizeObserver;

/** Starts the store, as the app does, then renders the Items tab. */
function showItems() {
  void init();
  return render(<ItemsView />);
}

afterEach(() => {
  cleanup();
  stop();
  backend.listeners = [];
  backend.popOutFail = undefined;
});

test("hidden sections stay hidden", async () => {
  showItems();
  expect(await screen.findByRole("region", { name: "Inventory" })).toBeTruthy();
  expect(screen.queryByRole("region", { name: "Songs" })).toBeNull();
});

test("missing items are grey, held ones show counts and tiers", async () => {
  showItems();
  const missing = await screen.findByRole("img", { name: "Hammer (missing)" });
  expect(missing.className).toContain("grayscale");
  const bombs = screen.getByRole("img", { name: "Bombs" });
  expect(bombs.className).not.toContain("grayscale");
  expect(within(bombs).getByText("20")).toBeTruthy();
  const bag = screen.getByRole("img", { name: "Bomb Bag" });
  expect(within(bag).getByText("2")).toBeTruthy();
});

test("a progressive Quest item shows its tier, as an upgrade does", async () => {
  const was = backend.state;
  backend.state = {
    ...was,
    cells: [cell("Magic", { section: "Quest", tier: 1, tiers: 2 })],
    options: {
      ...was.options,
      sections: [{ section: "Quest", column: 1, width: 1, shown: true }],
    },
  };
  try {
    showItems();
    const magic = await screen.findByRole("img", { name: "Magic" });
    expect(within(magic).getByText("1")).toBeTruthy();
  } finally {
    backend.state = was;
  }
});

test("a dungeon's row fills from the backend, names its dungeon and fades what it has no such item for", async () => {
  const dungeon = (label: string, patch: Partial<Cell> = {}) =>
    cell(label, { section: "Dungeons", ...patch });
  const state = (keys: number): ItemsState => ({
    cells: [
      dungeon("Deku Tree Small Keys", {
        tier: 0,
        tiers: 0,
        row: { short: "DT", name: "Deku Tree" },
      }),
      dungeon("Deku Tree Map"),
      dungeon("Forest Temple Small Keys", {
        tier: keys > 0 ? 1 : 0,
        count: keys,
        row: { short: "FoT", name: "Forest Temple" },
      }),
      dungeon("Forest Temple Boss Key", { tier: 0 }),
    ],
    options: {
      ...backend.state.options,
      sections: [{ section: "Dungeons", column: 1, width: 2, shown: true }],
    },
    game_art: true,
  });
  const was = backend.state;
  backend.state = state(3);
  try {
    showItems();
    const row = await screen.findByRole("region", { name: "Dungeons" });
    const keys = within(row).getByRole("img", {
      name: "Forest Temple Small Keys",
    });
    expect(within(keys).getByText("3")).toBeTruthy();
    const boss = within(row).getByRole("img", {
      name: "Forest Temple Boss Key (missing)",
    });
    expect(boss.className).toContain("grayscale");
    expect(
      within(row).getByRole("img", { name: "Deku Tree Map" }),
    ).toBeTruthy();
    // Each row starts with its dungeon's short name, the full one on hover.
    const grid = row.querySelector(".grid");
    expect(grid?.children).toHaveLength(6);
    expect(grid?.children[0]?.textContent).toBe("DT");
    fireEvent.focus(within(row).getByText("FoT"));
    expect((await screen.findByRole("tooltip")).textContent).toBe(
      "Forest Temple",
    );
    // A dungeon without the item still draws it, fainter than missing.
    const absent = within(row).getByRole("img", {
      name: "Deku Tree Small Keys (not in this dungeon)",
    });
    expect(absent.className).toContain("opacity-12");
    expect(absent.className).not.toContain("opacity-28");
    expect(row.querySelectorAll("img")).toHaveLength(4);
    // Rows line up with dungeons at one width, so it has no stepper.
    expect(within(row).getByRole("button", { name: "Hide Dungeons" }));
    expect(
      within(row).queryByRole("button", { name: "Wider Dungeons" }),
    ).toBeNull();

    backend.send(state(2));
    expect(
      within(
        within(row).getByRole("img", { name: "Forest Temple Small Keys" }),
      ).getByText("2"),
    ).toBeTruthy();
    backend.send(state(0));
    expect(
      within(row).getByRole("img", {
        name: "Forest Temple Small Keys (missing)",
      }),
    ).toBeTruthy();
  } finally {
    backend.state = was;
  }
});

/** A tile's own icon, the stand-in drawn without the dump's. */
const standIn = (tile: HTMLElement) => tile.querySelector("svg.lucide");

test("an icon that doesn't load falls back to zootr-nx's own", async () => {
  showItems();
  const bombs = await screen.findByRole("img", { name: "Bombs" });
  await waitFor(() => expect(bombs.querySelector("img")).toBeTruthy());
  act(() => bombs.querySelector("img")?.dispatchEvent(new Event("error")));
  expect(bombs.querySelector("img")).toBeNull();
  expect(standIn(bombs)).toBeTruthy();
});

test("with game assets off, tiles draw zootr-nx's icons and build nothing", async () => {
  const icons = vi.fn(() => backend.icons);
  const was = { state: backend.state, build: backend.build };
  backend.state = { ...was.state, game_art: false };
  backend.build = icons;
  try {
    showItems();
    const bombs = await screen.findByRole("img", { name: "Bombs" });
    expect(standIn(bombs)).toBeTruthy();
    expect(bombs.querySelector("img")).toBeNull();
    expect(screen.queryByText(/Building icons/)).toBeNull();
    expect(icons).not.toHaveBeenCalled();

    // Turned on, every window builds the dump's icons and draws them.
    backend.send({ ...backend.state, game_art: true });
    await waitFor(() => expect(bombs.querySelector("img")).toBeTruthy());
    expect(icons).toHaveBeenCalledOnce();
    expect(useItems.getState().art).toBe("game");

    // Turned off again, back to zootr-nx's.
    backend.send({ ...backend.state, game_art: false });
    expect(standIn(bombs)).toBeTruthy();
  } finally {
    backend.state = was.state;
    backend.build = was.build;
  }
});

test("a failed icon build falls back to zootr-nx's icons and says why", async () => {
  const was = backend.icons;
  backend.icons = Promise.resolve({ status: "error", error: "No icons" });
  try {
    showItems();
    const bombs = await screen.findByRole("img", { name: "Bombs" });
    expect(await screen.findByText("No icons")).toBeTruthy();
    expect(useItems.getState().art).toBe("fallback");
    expect(standIn(bombs)).toBeTruthy();
  } finally {
    backend.icons = was;
  }
});

test("the pop out renders the same grid and follows events", async () => {
  render(<TrackerWindow />);
  expect(
    await screen.findByRole("img", { name: "Hammer (missing)" }),
  ).toBeTruthy();
  expect(screen.queryByRole("region", { name: "Songs" })).toBeNull();
  expect(screen.queryByRole("toolbar")).toBeNull();
  expect(document.body.style.background).toContain("0, 255, 0");

  const hammer = cell("Hammer");
  backend.send({ ...backend.state, cells: [hammer] });
  expect(screen.getByRole("img", { name: "Hammer" })).toBeTruthy();
});

test("tiles wait on the icon build, with a status line", async () => {
  let built = (_: unknown) => {};
  backend.icons = new Promise((done) => {
    built = done;
  });
  showItems();
  const bombs = await screen.findByRole("img", { name: "Bombs" });
  expect(screen.getByText(/Building icons/)).toBeTruthy();
  expect(bombs.querySelector("img")).toBeNull();

  await act(async () => built({ status: "ok", data: null }));
  expect(screen.queryByText(/Building icons/)).toBeNull();
  expect(bombs.querySelector("img")).toBeTruthy();
});

test("a banner says the icons are zootr-nx's own until dismissed for good", async () => {
  const was = backend.state;
  backend.state = { ...was, game_art: false };
  useUi.setPath("items.bannerDismissed", false);
  try {
    showItems();
    const banner = await screen.findByText(/Showing zootr-nx's own icons/);
    fireEvent.click(screen.getByRole("button", { name: "Open Settings" }));
    expect(useUi.getState().tab).toBe("Settings");
    expect(useUi.getState().section).toBe("settings");

    fireEvent.click(screen.getByRole("button", { name: "Dismiss" }));
    expect(useUi.getState().items.bannerDismissed).toBe(true);
    expect(banner.isConnected).toBe(false);
  } finally {
    backend.state = was;
  }
});

test("the banner stays away while the game's icons draw", async () => {
  useUi.setPath("items.bannerDismissed", false);
  showItems();
  await waitFor(() => expect(useItems.getState().art).toBe("game"));
  expect(screen.queryByText(/Showing zootr-nx's own icons/)).toBeNull();
});

const region = (name: string) => screen.getByRole("region", { name });
const press = (name: string) =>
  fireEvent.click(screen.getByRole("button", { name }));

test("a section moved to another column draws there", async () => {
  showItems();
  await screen.findByRole("region", { name: "Upgrades" });
  expect(region("Upgrades").parentElement).toBe(
    region("Inventory").parentElement,
  );
  press("Move Upgrades right");
  expect(region("Upgrades").parentElement).not.toBe(
    region("Inventory").parentElement,
  );
});

test("width sets icons per row, up to the section's items", async () => {
  showItems();
  await screen.findByRole("region", { name: "Inventory" });
  const grid = () => region("Inventory").querySelector(".grid") as HTMLElement;
  expect(grid().style.gridTemplateColumns).toContain("repeat(2,");
  expect(
    (
      screen.getByRole("button", {
        name: "Wider Inventory",
      }) as HTMLButtonElement
    ).disabled,
  ).toBe(true);
  press("Narrower Inventory");
  expect(grid().style.gridTemplateColumns).toContain("repeat(1,");
});

test("up and down reorder a column, hide and show round trip", async () => {
  showItems();
  await screen.findByRole("region", { name: "Upgrades" });
  const order = () =>
    screen.getAllByRole("region").map((r) => r.getAttribute("aria-label"));
  expect(order()).toEqual(["Inventory", "Upgrades"]);
  press("Move Upgrades up");
  expect(order()).toEqual(["Upgrades", "Inventory"]);
  press("Hide Inventory");
  expect(order()).toEqual(["Upgrades"]);
  press("Show Inventory");
  expect(order()).toEqual(["Upgrades", "Inventory"]);
});

test("an option changed just before leaving the tab is still saved", async () => {
  showItems();
  const colour = await screen.findByDisplayValue("#00ff00");
  fireEvent.change(colour, { target: { value: "#123456" } });
  cleanup();
  await waitFor(() =>
    expect(itemsOptions).toHaveBeenCalledWith(
      expect.objectContaining({ background: "#123456" }),
    ),
  );
});

test("says why the tracker couldn't pop out", async () => {
  backend.popOutFail = "No window";
  showItems();
  fireEvent.click(await screen.findByRole("button", { name: "Pop Out" }));
  expect(await screen.findByText("No window")).toBeTruthy();
});
