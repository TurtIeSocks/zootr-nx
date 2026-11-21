import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { commands } from "@/bindings";
import { full, NEED_DUMP } from "@/lib/config";
import { Welcome } from "@/shell/welcome";
import { useConfig } from "@/stores/config";
import { useSeeds } from "@/stores/seeds";
import { useUi } from "@/stores/ui";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useUi.setPath("welcome.done", false);
});

const done = () =>
  screen.getByRole("button", { name: "Done" }) as HTMLButtonElement;
/** Whether Done holds its click, as a button that says why does. */
const held = () => done().getAttribute("aria-disabled") === "true";
const type = (label: string, value: string) =>
  fireEvent.change(screen.getByLabelText(label), { target: { value } });

test("Done says which of the dump and the target is missing", async () => {
  useConfig.setPath("config", full({}));
  useSeeds.setPath("status", { game: false, targets: [] });
  render(<Welcome />);
  fireEvent.focus(done());
  expect((await screen.findByRole("tooltip")).textContent).toBe(
    `${NEED_DUMP}\nEach target needs a name, an app and a mod folder`,
  );
  cleanup();

  // With the dump in, only the target is left.
  useSeeds.setPath("status", { game: true, targets: [] });
  render(<Welcome />);
  fireEvent.focus(done());
  expect((await screen.findByRole("tooltip")).textContent).toBe(
    "Each target needs a name, an app and a mod folder",
  );
});

test("Done waits for a dump and a target, then saves the target on Play", async () => {
  const setConfig = vi
    .spyOn(commands, "setConfig")
    .mockResolvedValue({ status: "ok", data: [] });
  useConfig.setPath("config", full({}));
  useSeeds.setPath("status", { game: false, targets: [] });
  useUi.setPath("tab", "Settings");
  render(<Welcome />);
  type("Name", "PC");
  type("App", "/a.app");
  type("Mod folder", "/m");
  expect(held()).toBe(true);

  useConfig.setPath("config.game", "/oot.nup");
  useSeeds.setPath("status", { game: true, targets: [] });
  expect(await screen.findByText(/Ready, with the 1.1.1 update/)).toBeTruthy();
  expect(held()).toBe(false);
  fireEvent.click(done());
  await vi.waitFor(() => expect(useUi.getState().welcome.done).toBe(true));
  expect(setConfig).toHaveBeenCalledWith(
    expect.objectContaining({
      picked: "PC",
      targets: [expect.objectContaining({ name: "PC", mod_dir: "/m" })],
    }),
  );
  expect(useUi.getState().tab).toBe("Play");
});
