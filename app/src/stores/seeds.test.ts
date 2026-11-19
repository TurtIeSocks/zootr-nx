import { expect, test, vi } from "vitest";
import { generate, useSeeds } from "@/stores/seeds";

const words = "Stopped: the targets are as they were";

vi.mock("@tauri-apps/api/core", () => ({ Channel: class {} }));
vi.mock("@/bindings", () => ({
  STEPS: [],
  commands: new Proxy(
    {},
    {
      get: (_, name) =>
        name === "generate"
          ? async () => ({ status: "error", error: words })
          : async () => ({ status: "ok", data: null }),
    },
  ),
}));

test("a stop answered before its last step event still reads as a stop", async () => {
  await generate(null);
  expect(useSeeds.getState().steps?.stopped).toBe(words);
  expect(useSeeds.getState().error).not.toBe(words);
});
