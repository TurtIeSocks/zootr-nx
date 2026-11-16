import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { keep } from "@/lib/api";
import { full } from "@/lib/config";
import { TargetsPane } from "@/settings/targets-pane";
import { useConfig } from "@/stores/config";

const importTarget = vi.fn((text: string) =>
  text.startsWith("{")
    ? Promise.resolve({
        status: "ok",
        data: { name: "Runner", app: "/a", mod_dir: "/m" },
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
  Promise.resolve({ status: "ok", data: '{"name":"Runner"}' }),
);
const dialog = {
  save: vi.fn(async () => "/shared/Runner.json" as string | null),
  open: vi.fn(async () => "/shared/Runner.json" as string | null),
};
vi.mock("@tauri-apps/plugin-dialog", () => ({
  save: () => dialog.save(),
  open: () => dialog.open(),
}));

/** Answers a config save; tests that hold one set `hold`. */
const saving = { hold: undefined as Promise<void> | undefined };
const setConfig = vi.fn(async () => {
  await saving.hold;
  return { status: "ok", data: null };
});

vi.mock("@/bindings", () => ({
  commands: {
    importTarget: (text: string) => importTarget(text),
    exportTarget: (name: string) => exportTarget(name),
    saveTarget: (name: string, file: string) => saveTarget(name, file),
    readTargetFile: (file: string) => readTargetFile(file),
    setConfig: () => setConfig(),
  },
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

const config = full({
  targets: [{ name: "Runner", app: "/a", mod_dir: "/m" }],
});

function show() {
  useConfig.setPath("config", config);
  render(<TargetsPane />);
}

const button = (name: string) =>
  screen.getByRole("button", { name }) as HTMLButtonElement;

const nameField = () => screen.getByDisplayValue(/^Runner/) as HTMLInputElement;

const rename = (value: string) =>
  fireEvent.change(nameField(), { target: { value } });

test("imports pasted text as a new draft with its name made unique", async () => {
  show();
  fireEvent.click(button("Import"));
  fireEvent.change(screen.getByLabelText("Target"), {
    target: { value: '{"name":"Runner"}' },
  });
  fireEvent.click(
    screen.getByRole("button", { name: "Import", hidden: false }),
  );
  await waitFor(() => expect(screen.getByText("Runner 2")).toBeTruthy());
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
  expect(screen.queryByText("Runner 2")).toBeNull();
});

test("copy puts the saved target's export on the clipboard", async () => {
  const writeText = vi.fn(() => Promise.resolve());
  Object.assign(navigator, { clipboard: { writeText } });
  show();
  fireEvent.click(button("Copy"));
  await waitFor(() =>
    expect(writeText).toHaveBeenCalledWith("preset of Runner"),
  );
});

test("a reload with the same targets keeps the form's edits, new ones reset it", () => {
  show();
  rename("Runner edited");
  act(() => useConfig.setPath("config", keep(full({ ...config }))));
  expect(nameField().value).toBe("Runner edited");
  act(() =>
    useConfig.setPath(
      "config",
      keep(full({ targets: [{ name: "Runner", app: "/b", mod_dir: "/m" }] })),
    ),
  );
  expect(nameField().value).toBe("Runner");
});

test("Save floats in a bar only while the targets have unsaved edits", () => {
  show();
  expect(screen.queryByRole("button", { name: "Save targets" })).toBeNull();
  rename("Runner edited");
  expect(screen.getByText("Unsaved changes to targets")).toBeTruthy();
  rename("Runner");
  expect(screen.queryByRole("button", { name: "Save targets" })).toBeNull();
});

test("Discard puts the saved targets back", () => {
  show();
  rename("Runner edited");
  fireEvent.click(button("Discard"));
  expect(nameField().value).toBe("Runner");
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
  rename("Runner edited");
  fireEvent.click(button("Save targets"));
  fireEvent.click(button("Save targets"));
  expect(setConfig).toHaveBeenCalledTimes(1);
  expect(button("Save targets").disabled).toBe(true);
  await act(async () => finish());
  await waitFor(() =>
    expect(screen.queryByRole("button", { name: "Save targets" })).toBeNull(),
  );
  expect(nameField().value).toBe("Runner edited");
  saving.hold = undefined;
});

test("Download writes the saved target to the file picked", async () => {
  show();
  fireEvent.click(button("Download"));
  await waitFor(() =>
    expect(saveTarget).toHaveBeenCalledWith("Runner", "/shared/Runner.json"),
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
      '{"name":"Runner"}',
    ),
  );
  expect(readTargetFile).toHaveBeenCalledWith("/shared/Runner.json");
});
