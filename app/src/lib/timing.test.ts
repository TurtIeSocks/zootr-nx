import { expect, test, vi } from "vitest";
import { commands } from "@/bindings";
import { timeCommands } from "@/lib/timing";

const appLogWrite = vi.fn((..._: unknown[]) => Promise.resolve());

vi.mock("@/bindings", () => ({
  commands: {
    devState: () => Promise.resolve({ status: "ok", data: 1 }),
    uninstall: () => Promise.resolve({ status: "error", error: "no dump" }),
    appLogWrite: (...args: unknown[]) => appLogWrite(...args),
  },
}));

let on = true;
timeCommands(() => on);

test("each call logs its time, keyed by command and outcome", async () => {
  expect(await commands.devState()).toEqual({ status: "ok", data: 1 });
  expect(appLogWrite).toHaveBeenLastCalledWith(
    "debug",
    expect.stringMatching(/^→ dev_state \d+\.\d\d s$/),
    "dev_state ok",
  );
  await commands.uninstall();
  expect(appLogWrite).toHaveBeenLastCalledWith(
    "debug",
    expect.stringMatching(/^→ uninstall \d+\.\d\d s, failed$/),
    "uninstall no dump",
  );
});

test("with developer tools off, nothing is logged", async () => {
  on = false;
  appLogWrite.mockClear();
  await commands.devState();
  expect(appLogWrite).not.toHaveBeenCalled();
  on = true;
});
