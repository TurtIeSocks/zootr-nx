import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { commands, type StepEvent } from "@/bindings";
import { generateKey } from "@/play/generate";
import { GenerateButton } from "@/play/generate-button";
import { stepped, useSeeds } from "@/stores/seeds";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  clearMocks();
  useSeeds.setState({
    blocked: "",
    unbuildable: "",
    busy: false,
    generating: false,
    stopping: false,
    steps: null,
  });
});

const button = (name: string | RegExp) =>
  screen.getByRole("button", { name }) as HTMLButtonElement;

/** Spies on generate, answering a seed. Its steps need the IPC. */
function spyGenerate() {
  mockIPC(() => {});
  return vi
    .spyOn(commands, "generate")
    .mockResolvedValue({ status: "error", error: "FakeStop" });
}

/** Opens the chevron's menu and picks `name`. */
async function pick(name: RegExp) {
  fireEvent.keyDown(button("More ways to generate"), { key: "Enter" });
  fireEvent.click(await screen.findByRole("menuitem", { name }));
}

test("the button installs, and the menu's Generate only doesn't", async () => {
  const generate = spyGenerate();
  render(<GenerateButton />);
  fireEvent.click(button(/Generate & install/));
  await waitFor(() =>
    expect(generate).toHaveBeenLastCalledWith(null, true, expect.anything()),
  );
  await waitFor(() => expect(useSeeds.getState().busy).toBe(false));
  await pick(/Generate only/);
  await waitFor(() =>
    expect(generate).toHaveBeenLastCalledWith(null, false, expect.anything()),
  );
});

test("⌘↵ installs and ⇧⌘↵ only generates", async () => {
  const generate = spyGenerate();
  render(
    // biome-ignore lint/a11y/noStaticElementInteractions: the folder's keys
    <div onKeyDown={generateKey}>
      <input aria-label="Seed" />
    </div>,
  );
  const seed = screen.getByRole("textbox", { name: "Seed" });
  fireEvent.keyDown(seed, { key: "Enter", metaKey: true });
  await waitFor(() =>
    expect(generate).toHaveBeenLastCalledWith(null, true, expect.anything()),
  );
  await waitFor(() => expect(useSeeds.getState().busy).toBe(false));
  fireEvent.keyDown(seed, { key: "Enter", metaKey: true, shiftKey: true });
  await waitFor(() =>
    expect(generate).toHaveBeenLastCalledWith(null, false, expect.anything()),
  );
  fireEvent.keyDown(seed, { key: "Enter" });
  expect(generate).toHaveBeenCalledTimes(2);
});

test("each part keeps its own blocked reason", async () => {
  const generate = spyGenerate();
  useSeeds.setState({ blocked: "Close the game first", unbuildable: "" });
  render(<GenerateButton />);
  const go = button(/Generate & install/);
  expect(go.getAttribute("aria-disabled")).toBe("true");
  expect(go.disabled).toBe(false);
  fireEvent.click(go);
  expect(generate).not.toHaveBeenCalled();
  fireEvent.keyDown(button("More ways to generate"), { key: "Enter" });
  const install = await screen.findByRole("menuitem", {
    name: /Generate & install/,
  });
  expect(install.textContent).toContain("Close the game first");
  expect(install.getAttribute("aria-disabled")).toBe("true");
  const only = screen.getByRole("menuitem", { name: /Generate only/ });
  expect(only.getAttribute("aria-disabled")).toBeNull();
});

test("while generating it shows the step and count, and Stop stops it", () => {
  const stop = vi.spyOn(commands, "stopGenerate").mockResolvedValue();
  const events: StepEvent[] = [
    { kind: "began", step: "dump" },
    { kind: "ended", step: "dump", detail: "cached", secs: 0.4 },
    { kind: "began", step: "checks" },
  ];
  const steps = events.reduce(stepped, {
    rows: [],
    end: null,
    secs: 0,
    total: 7,
  });
  useSeeds.setState({ generating: true, steps });
  render(<GenerateButton />);
  const run = button("Stop");
  expect(run.textContent).toContain("Reading checks…");
  expect(run.textContent).toContain("2 / 7");
  expect(button("More ways to generate").disabled).toBe(true);
  fireEvent.click(run);
  expect(stop).toHaveBeenCalledOnce();
  expect(button("Stopping").disabled).toBe(true);
});
