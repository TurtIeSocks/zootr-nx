import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { Build } from "@/bindings";
import { keep } from "@/lib/api";
import { type FullConfig, full } from "@/lib/config";
import { TargetsPane } from "@/settings/targets-pane";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";

const importTarget = vi.fn((text: string) =>
  text.startsWith("{")
    ? Promise.resolve({
        status: "ok",
        data: { name: "FakeTarget", app: "/a", mod_dir: "/m" },
      })
    : Promise.resolve({ status: "error", error: 'unknown key "bad"' }),
);
const exportTarget = vi.fn((name: string) =>
  Promise.resolve({ status: "ok", data: `preset of ${name}` }),
);

const saveTarget = vi.fn((_name: string, _file: string) =>
  Promise.resolve({ status: "ok", data: null }),
);
const readTargetFile = vi.fn((_file: string) =>
  Promise.resolve({ status: "ok", data: '{"name":"FakeTarget"}' }),
);
const dialog = {
  save: vi.fn(async () => "/shared/FakeTarget.json" as string | null),
  open: vi.fn(async () => "/shared/FakeTarget.json" as string | null),
};
vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: () => dialog.save(),
  open: () => dialog.open(),
}));

/** Answers a config save; tests that hold one set `hold`, and one that
 * moves a mod folder sets `notes` and what the config reads back. */
const saving = {
  hold: undefined as Promise<void> | undefined,
  notes: [] as string[],
  after: undefined as FullConfig | undefined,
};
const setConfig = vi.fn(async (_config?: FullConfig) => {
  await saving.hold;
  return { status: "ok", data: saving.notes };
});

const stopGenerate = vi.fn(async () => {});

vi.mock("@/bindings", () => ({
  commands: {
    stopGenerate: () => stopGenerate(),
    importTarget: (text: string) => importTarget(text),
    exportTarget: (name: string) => exportTarget(name),
    saveTarget: (name: string, file: string) => saveTarget(name, file),
    readTargetFile: (file: string) => readTargetFile(file),
    setConfig: (c: FullConfig) => setConfig(c),
    config: () => Promise.resolve({ status: "ok", data: saving.after }),
  },
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  useSeeds.setPath("status", { game: true, targets: [] });
});

const config = full({
  targets: [{ name: "FakeTarget", app: "/a", mod_dir: "/m" }],
});

function show() {
  useConfig.setPath("config", config);
  render(<TargetsPane />);
}

const button = (name: string) =>
  screen.getByRole("button", { name }) as HTMLButtonElement;

const nameField = () =>
  screen.getByDisplayValue(/^FakeTarget/) as HTMLInputElement;

const rename = (value: string) =>
  fireEvent.change(nameField(), { target: { value } });

test("imports pasted text as a new draft with its name made unique", async () => {
  show();
  fireEvent.click(button("Import"));
  fireEvent.change(screen.getByLabelText("Target"), {
    target: { value: '{"name":"FakeTarget"}' },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Import", hidden: false }),
  );
  await waitFor(() => expect(screen.getByText("FakeTarget 2")).toBeTruthy());
});

test("an import error shows in the dialog and adds nothing", async () => {
  show();
  fireEvent.click(button("Import"));
  fireEvent.change(screen.getByLabelText("Target"), {
    target: { value: "nope" },
  });
  const buttons = screen.getAllByRole("button", { name: "Import" });
  fireEvent.click(buttons[buttons.length - 1] as HTMLElement);
  await waitFor(() =>
    expect(screen.getByText(/unknown key "bad"/)).toBeTruthy(),
  );
  expect(screen.queryByText("FakeTarget 2")).toBeNull();
});

test("copy puts the saved target's export on the clipboard", async () => {
  const writeText = vi.fn(() => Promise.resolve());
  Object.assign(navigator, { clipboard: { writeText } });
  show();
  fireEvent.click(button("Copy"));
  await waitFor(() =>
    expect(writeText).toHaveBeenCalledWith("preset of FakeTarget"),
  );
});

test("a reload with the same targets keeps the form's edits, new ones reset it", () => {
  show();
  rename("FakeTarget edited");
  act(() => useConfig.setPath("config", keep(full({ ...config }))));
  expect(nameField().value).toBe("FakeTarget edited");
  act(() =>
    useConfig.setPath(
      "config",
      keep(
        full({ targets: [{ name: "FakeTarget", app: "/b", mod_dir: "/m" }] }),
      ),
    ),
  );
  expect(nameField().value).toBe("FakeTarget");
});

test("Save floats in a bar only while the targets have unsaved edits", () => {
  show();
  expect(screen.queryByRole("button", { name: "Save targets" })).toBeNull();
  rename("FakeTarget edited");
  expect(screen.getByText("Unsaved changes to targets")).toBeTruthy();
  rename("FakeTarget");
  expect(screen.queryByRole("button", { name: "Save targets" })).toBeNull();
});

test("Discard puts the saved targets back", () => {
  show();
  rename("FakeTarget edited");
  fireEvent.click(button("Discard"));
  expect(nameField().value).toBe("FakeTarget");
  expect(screen.queryByRole("button", { name: "Discard" })).toBeNull();
});

test("an invalid draft says why in the bar", () => {
  show();
  rename("");
  fireEvent.click(button("Save targets"));
  expect(screen.getByRole("alert")).toBeTruthy();
  expect(setConfig).not.toHaveBeenCalled();
});

test("a second Save while one runs is ignored, and the bar goes once saved", async () => {
  let finish = () => {};
  saving.hold = new Promise((resolve) => (finish = resolve));
  show();
  rename("FakeTarget edited");
  fireEvent.click(button("Save targets"));
  fireEvent.click(button("Save targets"));
  expect(setConfig).toHaveBeenCalledTimes(1);
  expect(button("Save targets").disabled).toBe(true);
  await act(async () => finish());
  await waitFor(() =>
    expect(screen.queryByRole("button", { name: "Save targets" })).toBeNull(),
  );
  expect(nameField().value).toBe("FakeTarget edited");
  saving.hold = undefined;
});

test("Download writes the saved target to the file picked", async () => {
  show();
  fireEvent.click(button("Download"));
  await waitFor(() =>
    expect(saveTarget).toHaveBeenCalledWith(
      "FakeTarget",
      "/shared/FakeTarget.json",
    ),
  );
});

test("Download does nothing when the picker is cancelled", async () => {
  dialog.save.mockResolvedValueOnce(null);
  show();
  fireEvent.click(button("Download"));
  await waitFor(() => expect(dialog.save).toHaveBeenCalled());
  expect(saveTarget).not.toHaveBeenCalled();
});

test("Import opens a file into the text box to check before importing", async () => {
  show();
  fireEvent.click(button("Import"));
  fireEvent.click(button("Open a file"));
  await waitFor(() =>
    expect((screen.getByLabelText("Target") as HTMLTextAreaElement).value).toBe(
      '{"name":"FakeTarget"}',
    ),
  );
  expect(readTargetFile).toHaveBeenCalledWith("/shared/FakeTarget.json");
});

test("a save that moves a mod folder says so, and shows the folder it saved", async () => {
  const moved = "/m/0400486024DEC000/zootr";
  const note = `FakeTarget: installs into ${moved}`;
  saving.notes = [note];
  saving.after = full({
    targets: [{ name: "FakeTarget", app: "/a", mod_dir: moved }],
  });
  show();
  rename("FakeTarget edited");
  fireEvent.click(button("Save targets"));
  await waitFor(() => expect(screen.getByText(note)).toBeTruthy());
  expect(screen.getByDisplayValue(moved)).toBeTruthy();
  saving.notes = [];
  saving.after = undefined;
});

/** Shows the pane with FakeTarget's game built as `build` says. */
function showBuilt(build: Build, running = false) {
  useSeeds.setPath("status", {
    game: true,
    targets: [{ name: "FakeTarget", app: true, mod_dir: true, running, build }],
  });
  show();
}

const stale =
  "FakeTarget needs a rebuild: its hook patch changed since the last build";

test("a stale build shows its badge, what's wrong and Rebuild", () => {
  showBuilt({ status: { kind: "stale" }, text: stale, command: true });
  expect(screen.getByText("Needs a rebuild")).toBeTruthy();
  expect(screen.getByText(stale)).toBeTruthy();
  expect(button("Rebuild").disabled).toBe(false);
  expect(screen.queryByText(/Rebuild it by hand/)).toBeNull();
});

test("Rebuild waits while the target's game runs", () => {
  showBuilt({ status: { kind: "stale" }, text: stale, command: true }, true);
  expect(button("Rebuild").getAttribute("aria-disabled")).toBe("true");
});

test("a stale build with no command says to rebuild it by hand", () => {
  showBuilt({ status: { kind: "stale" }, text: stale, command: false });
  expect(screen.getByText(/Rebuild it by hand/)).toBeTruthy();
  expect(screen.queryByRole("button", { name: "Rebuild" })).toBeNull();
});

test("a current build only shows its badge", () => {
  showBuilt({ status: { kind: "current" }, text: null, command: true });
  expect(screen.getByText("Build current")).toBeTruthy();
  expect(screen.queryByText(stale)).toBeNull();
});

test("Stop shows while this target rebuilds and stops it", () => {
  useSeeds.setPath("generating", true);
  useSeeds.setPath("rebuilt", "FakeTarget");
  showBuilt({ status: { kind: "stale" }, text: stale, command: true });
  expect(button("Rebuild").disabled).toBe(true);
  fireEvent.click(button("Stop"));
  expect(stopGenerate).toHaveBeenCalledOnce();
  expect(button("Stopping").disabled).toBe(true);
});

test("a card shows four fields, the rest under a closed Advanced", () => {
  show();
  const advanced = screen.getByText("Advanced").closest("details");
  expect(advanced?.open).toBe(false);
  for (const label of ["Name", "App", "Mod folder", "Save folder (optional)"])
    expect(advanced?.contains(screen.getByLabelText(label))).toBe(false);
  expect(
    advanced?.contains(screen.getByLabelText("Launch arguments (optional)")),
  ).toBe(true);
  expect(screen.getByRole("button", { name: "About Targets" })).toBeTruthy();
  expect(screen.getByRole("button", { name: "About Mod folder" })).toBeTruthy();
});

test("a target using an advanced field or a variable opens both", () => {
  useConfig.setPath(
    "config",
    full({
      targets: [
        { name: "FakeTarget", app: "/a", mod_dir: "/m", env: ["S={bridge}"] },
      ],
    }),
  );
  render(<TargetsPane />);
  expect(screen.getByText("Advanced").closest("details")?.open).toBe(true);
  const variables = screen.getByText("Variables", { selector: "summary" });
  expect(variables.closest("details")?.open).toBe(true);
  expect(screen.getByText("{game}")).toBeTruthy();
});

test("scripted-play fields and their variables show only with developer tools", () => {
  const scripted = () => [
    screen.queryByLabelText("Harness environment (optional)"),
    screen.queryByLabelText("Control file (optional)"),
    screen.queryByText("{save}"),
    screen.queryByText("{control}"),
  ];
  show();
  expect(scripted()).toEqual([null, null, null, null]);
  act(() => useConfig.setPath("config.dev_tools", true));
  expect(scripted().every(Boolean)).toBe(true);
});

test("the rebuild fields show only with developer tools, and a saved one survives hidden", async () => {
  const rebuild = () => [
    screen.queryByLabelText("Rebuild command (optional)"),
    screen.queryByLabelText("Patch list (optional)"),
  ];
  useConfig.setPath(
    "config",
    full({
      targets: [
        {
          name: "FakeTarget",
          app: "/a",
          mod_dir: "/m",
          rebuild: { command: ["make"], manifest: "/p.txt" },
        },
      ],
    }),
  );
  render(<TargetsPane />);
  expect(rebuild()).toEqual([null, null]);
  expect(screen.getByText("Launch arguments", { selector: "td" })).toBeTruthy();
  rename("FakeTarget2");
  fireEvent.click(button("Save targets"));
  await waitFor(() => expect(setConfig).toHaveBeenCalled());
  expect(setConfig.mock.calls[0]?.[0]?.targets[0]?.rebuild).toEqual({
    command: ["make"],
    manifest: "/p.txt",
  });
  act(() => useConfig.setPath("config.dev_tools", true));
  expect(rebuild().every(Boolean)).toBe(true);
  expect(screen.getByText("Launch arguments, Rebuild command")).toBeTruthy();
});
