import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type {
  ChecksEvent,
  ChecksState,
  DoorRow,
  DoorsEvent,
  Mark,
  Summary,
} from "@/bindings";
import { ChecksView } from "@/checks/checks-view";
import { row } from "@/checks/fixture";
import { init } from "@/stores/checks";
import { label } from "@/stores/live";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

interface Ok {
  status: "ok";
  data: unknown;
}

const ok = (data: unknown): Promise<Ok> =>
  Promise.resolve({ status: "ok", data });

const backend = {
  listener: undefined as ((e: { payload: ChecksEvent }) => void) | undefined,
  doorsListener: undefined as
    | ((e: { payload: DoorsEvent }) => void)
    | undefined,
  state: {
    id: 7,
    rows: [
      row("1", "Kokiri Forest", "Front left"),
      row("2", "Kokiri Forest", "Back left", { checked: true, flag: 2 }),
      row("3", "Kokiri Forest", "Grotto", { flag: null }),
    ],
    doors: [],
  } as ChecksState,
  send(payload: ChecksEvent) {
    act(() => backend.listener?.({ payload }));
  },
};

const checksState = vi.fn(() => ok(backend.state));

const checksMark = vi.fn(
  (
    _id: number,
    _hash: string,
    _mark: Mark | null,
  ): Promise<
    { status: "ok"; data: null } | { status: "error"; error: string }
  > => Promise.resolve({ status: "ok", data: null }),
);

const checksPick = vi.fn(
  (_id: number, _door: string, _dungeon: string | null) =>
    Promise.resolve({ status: "ok", data: null }),
);

const liveLabel = vi.fn((_hash: number, _label: string) =>
  Promise.resolve({ status: "ok", data: null }),
);

vi.mock("@/bindings", () => ({
  events: {
    checksEvent: {
      listen: (cb: (e: { payload: ChecksEvent }) => void) => {
        backend.listener = cb;
        return Promise.resolve(() => {});
      },
    },
    doorsEvent: {
      listen: (cb: (e: { payload: DoorsEvent }) => void) => {
        backend.doorsListener = cb;
        return Promise.resolve(() => {});
      },
    },
  },
  commands: {
    checksState: () => checksState(),
    checksMark: (id: number, hash: string, mark: Mark | null) =>
      checksMark(id, hash, mark),
    checksPick: (id: number, door: string, dungeon: string | null) =>
      checksPick(id, door, dungeon),
    liveLabel: (hash: number, label: string) => liveLabel(hash, label),
  },
}));

afterEach(cleanup);

async function show() {
  await act(init);
  render(<ChecksView />);
}

test("items stay hidden until checked, and a flip shows them", async () => {
  await show();
  await screen.findByText("Kokiri Forest");
  expect(screen.getByText("1/3")).toBeTruthy();
  expect(screen.getByText("Item in Back left")).toBeTruthy();
  expect(screen.queryByText("Item in Front left")).toBeNull();
  backend.send({ hash: "1", checked: true });
  expect(screen.getByText("Item in Front left")).toBeTruthy();
  expect(screen.getByText("2/3")).toBeTruthy();
});

test("the filter shows what's left, then only marked checks", async () => {
  await show();
  await screen.findByText("Kokiri Forest");
  const remaining = screen.getByRole("button", { name: "Remaining" });
  fireEvent.click(remaining);
  expect(remaining.getAttribute("aria-pressed")).toBe("true");
  expect(screen.queryByText("Back left")).toBeNull();
  expect(screen.getByText("Front left")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Marked" }));
  expect(screen.queryByText("Kokiri Forest")).toBeNull();
});

test("a note is saved on the seed and shown", async () => {
  await show();
  fireEvent.pointerDown(await screen.findByLabelText("Mark Front left"), {
    button: 0,
    ctrlKey: false,
  });
  fireEvent.click(await screen.findByText("Note…"));
  fireEvent.change(await screen.findByLabelText("Note"), {
    target: { value: " behind the bed " },
  });
  fireEvent.click(screen.getByText("Save"));
  const note = { kind: "Note", text: "behind the bed" };
  await waitFor(() => expect(checksMark).toHaveBeenCalledWith(7, "1", note));
  expect(await screen.findByText("behind the bed")).toBeTruthy();
});

const openMenu = async (name: string) =>
  fireEvent.pointerDown(await screen.findByLabelText(`Mark ${name}`), {
    button: 0,
    ctrlKey: false,
  });

test("renaming labels the check's flag and the label wins", async () => {
  await show();
  await openMenu("Front left");
  fireEvent.click(await screen.findByText("Rename…"));
  fireEvent.change(await screen.findByLabelText("Name"), {
    target: { value: "By the door " },
  });
  fireEvent.click(screen.getByText("Save"));
  await waitFor(() => expect(liveLabel).toHaveBeenCalledWith(1, "By the door"));
  expect(await screen.findByText("By the door")).toBeTruthy();
});

test("a check with no known flag can't be renamed", async () => {
  await show();
  await openMenu("Grotto");
  await screen.findByText("Note…");
  expect(screen.queryByText("Rename…")).toBeNull();
});

test("the filter and search are still set after leaving the tab", async () => {
  await show();
  fireEvent.click(await screen.findByRole("button", { name: "Remaining" }));
  fireEvent.change(screen.getByPlaceholderText("Filter checks"), {
    target: { value: "front" },
  });
  cleanup();
  render(<ChecksView />);
  expect(
    screen
      .getByRole("button", { name: "Remaining" })
      .getAttribute("aria-pressed"),
  ).toBe("true");
  expect(
    (screen.getByPlaceholderText("Filter checks") as HTMLInputElement).value,
  ).toBe("front");
  expect(useUi.getState().checks).toEqual({
    query: "front",
    filter: "remaining",
  });
});

test("a new seed loads its checks, keeping the old ones until they come", async () => {
  await show();
  await screen.findByText("Front left");
  let send = (_: Ok) => {};
  checksState.mockReturnValueOnce(
    new Promise((resolve) => {
      send = resolve;
    }),
  );
  act(() =>
    useSeeds.setPath("history.installed", { ...({} as Summary), id: 8 }),
  );
  expect(checksState).toHaveBeenCalledTimes(2);
  expect(screen.getByText("Front left")).toBeTruthy();
  await act(async () =>
    send({
      status: "ok",
      data: { id: 8, rows: [row("9", "Lost Woods", "Skull kid")], doors: [] },
    }),
  );
  expect(screen.getByText("Skull kid")).toBeTruthy();
  expect(screen.queryByText("Front left")).toBeNull();
});

test("a label set in Live renames the check too", async () => {
  await show();
  await screen.findByText("Front left");
  await act(() => label(1, "By the door "));
  expect(liveLabel).toHaveBeenCalledWith(1, "By the door ");
  expect(screen.getByText("By the door")).toBeTruthy();
});

test("a mark that fails says why in its own row", async () => {
  await show();
  checksMark.mockResolvedValueOnce({ status: "error", error: "Disk full" });
  await openMenu("Front left");
  fireEvent.click(await screen.findByText("Skip"));
  const alert = await screen.findByRole("alert");
  expect(alert.textContent).toBe("Disk full");
  expect(alert.closest("tr")?.textContent).toContain("Front left");
});

test("a note being typed stays open when its check ticks out of the filter", async () => {
  await show();
  await screen.findByText("Kokiri Forest");
  fireEvent.click(screen.getByRole("button", { name: "Remaining" }));
  fireEvent.pointerDown(screen.getByLabelText("Mark Front left"), {
    button: 0,
    ctrlKey: false,
  });
  fireEvent.click(await screen.findByText("Note…"));
  fireEvent.change(await screen.findByLabelText("Note"), {
    target: { value: "under the sign" },
  });
  backend.send({ hash: "1", checked: true });
  expect(screen.queryByText("Front left")).toBeNull();
  expect((screen.getByLabelText("Note") as HTMLInputElement).value).toBe(
    "under the sign",
  );
});

/** A shuffled door for tests, hidden and unpicked unless `patch` says. */
const door = (id: string, vanilla: string, patch: Partial<DoorRow> = {}) => ({
  id,
  name: `FakeArea, ${vanilla} door`,
  vanilla,
  found: null,
  pick: null,
  ...patch,
});

test("doors show ? until found, a pick until then, and found over a pick", async () => {
  backend.state = {
    ...backend.state,
    doors: [
      door("deku-tree", "Deku Tree"),
      door("fire-temple", "Fire Temple", { pick: "deku-tree" }),
    ],
  };
  await show();
  expect(await screen.findByText("Entrances")).toBeTruthy();
  expect(screen.getByText("0/2")).toBeTruthy();
  expect(screen.getByText("?")).toBeTruthy();
  // The pick names its dungeon by its vanilla door.
  const rows = screen.getAllByRole("row");
  expect(
    rows.some((r) => r.textContent?.includes("Fire Temple doorDeku Tree")),
  ).toBe(true);
  act(() =>
    backend.doorsListener?.({
      payload: {
        doors: [
          door("deku-tree", "Deku Tree", { found: "fire-temple" }),
          door("fire-temple", "Fire Temple", { pick: "deku-tree" }),
        ],
      },
    }),
  );
  expect(await screen.findByText("1/2")).toBeTruthy();
  expect(screen.queryByText("?")).toBeNull();
  expect(screen.queryByLabelText("Pick FakeArea, Deku Tree door")).toBeNull();
  backend.state = { ...backend.state, doors: [] };
});

test("a pick is saved on the seed and shown", async () => {
  backend.state = {
    ...backend.state,
    doors: [door("deku-tree", "Deku Tree"), door("fire-temple", "Fire Temple")],
  };
  await show();
  fireEvent.pointerDown(
    await screen.findByLabelText("Pick FakeArea, Deku Tree door"),
    { button: 0, ctrlKey: false },
  );
  fireEvent.click(await screen.findByRole("menuitem", { name: "Fire Temple" }));
  await waitFor(() =>
    expect(checksPick).toHaveBeenCalledWith(7, "deku-tree", "fire-temple"),
  );
  await waitFor(() => expect(screen.getAllByText("?")).toHaveLength(1));
  backend.state = { ...backend.state, doors: [] };
});
