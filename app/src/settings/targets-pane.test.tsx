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
    setConfig: () => setConfig(),
  },
}));

afterEach(cleanup);

const config = full({
  targets: [{ name: "Runner", app: "/a", mod_dir: "/m" }],
});

function show() {
  useConfig.setPath("config", config);
  render(<TargetsPane />);
}

const button = (name: string) =>
  screen.getByRole("button", { name }) as HTMLButtonElement;

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
  const name = () => screen.getByDisplayValue(/^Runner/) as HTMLInputElement;
  fireEvent.change(name(), { target: { value: "Runner edited" } });
  act(() => useConfig.setPath("config", keep(full({ ...config }))));
  expect(name().value).toBe("Runner edited");
  act(() =>
    useConfig.setPath(
      "config",
      keep(full({ targets: [{ name: "Runner", app: "/b", mod_dir: "/m" }] })),
    ),
  );
  expect(name().value).toBe("Runner");
});

test("a second Save while one runs is ignored", async () => {
  let finish = () => {};
  saving.hold = new Promise((resolve) => (finish = resolve));
  show();
  fireEvent.click(button("Save targets"));
  fireEvent.click(button("Save targets"));
  expect(setConfig).toHaveBeenCalledTimes(1);
  expect(button("Save targets").disabled).toBe(true);
  await act(async () => finish());
  await waitFor(() => expect(button("Save targets").disabled).toBe(false));
  saving.hold = undefined;
});
