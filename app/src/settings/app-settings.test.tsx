import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { commands } from "@/bindings";
import { full } from "@/lib/config";
import { AppSettings } from "@/settings/app-settings";
import { useConfig } from "@/stores/config";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

test("the page keeps the game dump and saves developer tools", async () => {
  const setConfig = vi
    .spyOn(commands, "setConfig")
    .mockResolvedValue({ status: "ok", data: [] });
  const config = full({ game: "/oot.nup" });
  useConfig.setPath("config", config);
  render(<AppSettings />);
  expect(
    screen.getByRole("heading", { level: 1, name: "Settings" }),
  ).toBeTruthy();
  expect((screen.getByLabelText("Game dump") as HTMLInputElement).value).toBe(
    "/oot.nup",
  );
  expect(screen.queryByRole("switch", { name: "Dev panel" })).toBeNull();
  fireEvent.click(screen.getByRole("switch", { name: "Developer tools" }));
  await vi.waitFor(() =>
    expect(setConfig).toHaveBeenCalledWith({ ...config, dev_tools: true }),
  );
});

test("game assets save from their switch, which needs a dump", async () => {
  const setConfig = vi
    .spyOn(commands, "setConfig")
    .mockResolvedValue({ status: "ok", data: [] });
  useConfig.setPath("config", full({}));
  render(<AppSettings />);
  const name = "Icons from the game dump";
  expect(
    screen.getByText(/doesn't include any of the game's art/),
  ).toBeTruthy();
  const toggle = screen.getByRole("switch", { name }) as HTMLButtonElement;
  expect(toggle.getAttribute("aria-checked")).toBe("false");
  expect(toggle.disabled).toBe(true);

  const config = full({ game: "/oot.nup" });
  act(() => useConfig.setPath("config", config));
  expect(toggle.disabled).toBe(false);
  fireEvent.click(toggle);
  await vi.waitFor(() =>
    expect(setConfig).toHaveBeenCalledWith({ ...config, game_assets: true }),
  );
});

test("a newly saved game dump resets the field to it", () => {
  useConfig.setPath("config", full({ game: "/oot.nup" }));
  render(<AppSettings />);
  const dump = () => screen.getByLabelText("Game dump") as HTMLInputElement;
  fireEvent.change(dump(), { target: { value: "/typed.nup" } });
  act(() => useConfig.setPath("config.game", "/other.nup"));
  expect(dump().value).toBe("/other.nup");
});

test("the update check switch saves its inverse", async () => {
  const setConfig = vi
    .spyOn(commands, "setConfig")
    .mockResolvedValue({ status: "ok", data: [] });
  const config = full({});
  useConfig.setPath("config", config);
  render(<AppSettings />);
  const toggle = screen.getByRole("switch", { name: "Check for updates" });
  expect(toggle.getAttribute("aria-checked")).toBe("true");
  fireEvent.click(toggle);
  await vi.waitFor(() =>
    expect(setConfig).toHaveBeenCalledWith({
      ...config,
      no_update_check: true,
    }),
  );
});
