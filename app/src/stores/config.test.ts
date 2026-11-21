import { expect, test, vi } from "vitest";
import type { Config, SettingDef } from "@/bindings";
import { full } from "@/lib/config";
import { changeSettings, useConfig } from "@/stores/config";
import { useUi } from "@/stores/ui";

/** Each save waits here until the test lets it finish. */
const saves: { config: Config; finish: () => void }[] = [];

vi.mock("@/bindings", () => ({
  commands: {
    setConfig: (config: Config) =>
      new Promise((resolve) =>
        saves.push({
          config,
          finish: () => resolve({ status: "ok", data: [] }),
        }),
      ),
  },
}));

test("a second change made while the first saves keeps the first", async () => {
  useConfig.setPath("config", full({}));
  const first = changeSettings({ a: true });
  const second = changeSettings({ b: true });
  await vi.waitFor(() => expect(saves).toHaveLength(1));
  saves[0]?.finish();
  await vi.waitFor(() => expect(saves).toHaveLength(2));
  saves[1]?.finish();
  await Promise.all([first, second]);
  expect(saves[1]?.config.settings).toEqual({ a: true, b: true });
  expect(useConfig.getState().config.settings).toEqual({ a: true, b: true });
});

test("a saved section whose panel is gone opens the first panel", () => {
  const def = (id: string, group: string): SettingDef => ({
    id,
    group,
    label: id,
    description: "",
    kind: { type: "toggle", default: false },
    parent: null,
    section: "Fake section",
    list: false,
  });
  useUi.setPath("section", "Hints");
  useConfig.setPath("schema", [def("a", "General"), def("b", "Cutscenes")]);
  expect(useUi.getState().section).toBe("General");
  // A panel that's still there, or an app pane, stays.
  useUi.setPath("section", "Cutscenes");
  useConfig.setPath("schema", [def("a", "General"), def("b", "Cutscenes")]);
  expect(useUi.getState().section).toBe("Cutscenes");
  useUi.setPath("section", "targets");
  useConfig.setPath("schema", [def("c", "General")]);
  expect(useUi.getState().section).toBe("targets");
});
