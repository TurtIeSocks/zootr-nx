import { afterEach, expect, test, vi } from "vitest";
import {
  backend,
  batched,
  COLUMNS,
  createUi,
  init,
  memory,
  useUi,
} from "@/stores/ui";

const backendFile = vi.hoisted(() => ({
  uiLoad: vi.fn(),
  uiSave: vi.fn(),
  shared: undefined as ((e: { payload: unknown }) => void) | undefined,
  onClose: undefined as (() => Promise<void>) | undefined,
}));

vi.mock("@/bindings", () => ({
  commands: { uiLoad: backendFile.uiLoad, uiSave: backendFile.uiSave },
  events: {
    uiShared: {
      listen: async (f: (e: { payload: unknown }) => void) => {
        backendFile.shared = f;
        return () => {};
      },
    },
  },
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    onCloseRequested: async (f: () => Promise<void>) => {
      backendFile.onClose = f;
      return () => {};
    },
  }),
}));

afterEach(() => {
  localStorage.clear();
  vi.clearAllMocks();
});

test("setPath writes through persist, and a store built from the saved JSON has it back", async () => {
  const storage = memory();
  const first = createUi(storage);
  first.setPath("live.columns.name", 200);
  first.setPath("tab", "Live");
  expect(storage.saved.get("ui")?.state.live.columns.name).toBe(200);

  const second = createUi(storage);
  await second.persist.rehydrate();
  expect(second.getPath("live.columns.name")).toBe(200);
  expect(second.getPath("tab")).toBe("Live");
});

test("a saved shape from an older version is upgraded", async () => {
  const storage = memory();
  storage.setItem("ui", {
    state: { page: "Checks" } as never,
    version: 0,
  });
  const ui = createUi(storage, [
    ({ page, ...rest }) => ({ ...rest, tab: page }),
  ]);
  await ui.persist.rehydrate();
  expect(ui.getPath("tab")).toBe("Checks");
  expect(ui.getState()).not.toHaveProperty("page");
});

test("a branch saved before a field existed gets that field's default", async () => {
  const storage = memory();
  storage.setItem("ui", {
    state: { live: { columns: { time: 60 } } } as never,
    version: 0,
  });
  const ui = createUi(storage);
  await ui.persist.rehydrate();
  expect(ui.getPath("live.columns")).toEqual({ ...COLUMNS, time: 60 });
  expect(ui.getPath("section")).toBe("settings");
});

test("the old localStorage keys are taken only when nothing is saved", async () => {
  localStorage.setItem("theme", "dark");
  localStorage.setItem("live-columns", JSON.stringify({ name: 300 }));
  const fresh = createUi(memory());
  await fresh.persist.rehydrate();
  expect(fresh.getPath("theme")).toBe("dark");
  expect(fresh.getPath("live.columns.name")).toBe(300);

  const storage = memory();
  createUi(storage).setPath("theme", "light");
  const saved = createUi(storage);
  await saved.persist.rehydrate();
  expect(saved.getPath("theme")).toBe("light");
});

test("an unknown old theme is left out", async () => {
  localStorage.setItem("theme", "sepia");
  const ui = createUi(memory());
  await ui.persist.rehydrate();
  expect(ui.getPath("theme")).toBe("system");
});

test("batched writes send the first at once, then the last of a beat", async () => {
  const storage = memory();
  const setItem = vi.spyOn(storage, "setItem");
  const file = batched(storage);
  const ui = createUi(file);
  ui.setPath("tab", "Items");
  expect(setItem).toHaveBeenCalledOnce();
  ui.setPath("tab", "Live");
  ui.setPath("tab", "Play");
  expect(setItem).toHaveBeenCalledOnce();
  await file.flush();
  expect(setItem).toHaveBeenCalledTimes(2);
  expect(storage.saved.get("ui")?.state.tab).toBe("Play");
});

test("a steady stream of changes still saves every beat", async () => {
  vi.useFakeTimers();
  try {
    const storage = memory();
    const setItem = vi.spyOn(storage, "setItem");
    const ui = createUi(batched(storage));
    for (let i = 0; i < 10; i++) {
      ui.setPath("tab", i % 2 ? "Live" : "Items");
      await vi.advanceTimersByTimeAsync(100);
    }
    // A debounce would have written nothing while changes kept coming.
    expect(setItem.mock.calls.length).toBeGreaterThanOrEqual(3);
  } finally {
    vi.useRealTimers();
  }
});

test("the backend keeps the theme in the shared part and lays it over the window's", async () => {
  backendFile.uiSave.mockResolvedValue({ status: "ok", data: null });
  await backend.setItem("ui", {
    state: { ...useUi.getInitialState(), theme: "dark" },
    version: 0,
  });
  const [own, shared] = backendFile.uiSave.mock.calls[0] ?? [];
  expect(shared).toEqual({ theme: "dark" });
  expect(own.state).not.toHaveProperty("theme");
  expect(own.state.tab).toBe("Play");

  backendFile.uiLoad.mockResolvedValue({
    status: "ok",
    data: { own: null, shared: { theme: "light" } },
  });
  expect(await backend.getItem("ui")).toEqual({
    state: { theme: "light" },
    version: 1,
  });
});

test("init drops the old keys, saves what it took, and follows the shared part", async () => {
  useUi.persist.clearStorage();
  localStorage.setItem("theme", "dark");
  await init();
  expect(localStorage.getItem("theme")).toBeNull();
  expect(useUi.getState().theme).toBe("dark");
  expect(useUi.persist.getOptions().storage?.getItem("ui")).toMatchObject({
    state: { theme: "dark" },
  });
  expect(backendFile.onClose).toBeDefined();

  backendFile.shared?.({ payload: { theme: "light" } });
  expect(useUi.getState().theme).toBe("light");
});
