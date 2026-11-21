import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { SettingDef } from "@/bindings";
import { SettingsView } from "@/settings/settings-view";
import { Find } from "@/shell/find/find";
import { Topbar } from "@/shell/page-header";
import { useConfig } from "@/stores/config";
import { useUi } from "@/stores/ui";
import { checkNow } from "@/stores/update";

vi.mock("@/stores/update", async (real) => ({
  ...(await real<typeof import("@/stores/update")>()),
  checkNow: vi.fn(() => Promise.resolve("current")),
}));

afterEach(cleanup);

const cows: SettingDef = {
  id: "fake-cows",
  group: "Shuffles",
  label: "Shuffle fake cows",
  description: "Each fake cow gives an item.",
  kind: { type: "toggle", default: false },
  parent: null,
  requires: null,
  section: "Fake section",
  section_about: "",
  list: false,
};

/** The top bar, the settings page while it's open, and the search. */
// biome-ignore lint/style/useComponentExportOnlyModules: a test file never hot reloads
function Harness() {
  const [tab] = useUi.usePath("tab");
  return (
    <>
      <Topbar />
      {tab === "Settings" && <SettingsView />}
      <Find />
    </>
  );
}

function renderApp() {
  useConfig.setPath("schema", [cows]);
  useConfig.setPath("groups", [{ name: "Shuffles" }]);
  render(<Harness />);
}

const dialog = () => screen.queryByRole("dialog", { name: "Search" });

/** Opens the search with ⌘K and types `query`. */
function find(query: string) {
  fireEvent.keyDown(window, { key: "k", metaKey: true });
  const field = screen.getByRole("textbox", { name: "Search" });
  fireEvent.change(field, { target: { value: query } });
  return field;
}

test("⌘K opens it, and the top bar's button too; Esc closes it", () => {
  renderApp();
  const field = find("");
  expect(dialog()).toBeTruthy();
  fireEvent.keyDown(field, { key: "Escape" });
  expect(dialog()).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Search" }));
  expect(dialog()).toBeTruthy();
});

test("with nothing typed it lists the recent picks still there", () => {
  useUi.setPath("find.recent", [
    { group: "pages", key: "Save" },
    { group: "seeds", key: "404" },
  ]);
  renderApp();
  find("");
  const recent = within(screen.getByRole("list", { name: "Recent" }));
  expect(recent.getAllByRole("button").map((b) => b.textContent)).toEqual([
    "Save EditorClose the game first: it writes over its saves as it plays. On Items, click raises, right-click or Backspace lowers. Presets wait for Save.",
  ]);
});

test("+N more scopes to its group, which with nothing typed lists it all", () => {
  useUi.setPath("find.recent", [{ group: "pages", key: "Save" }]);
  renderApp();
  const field = find("e");
  expect(screen.getByRole("list", { name: "Settings" })).toBeTruthy();
  const [pagesMore] = screen.getAllByRole("button", { name: /^\+\d+ more$/ });
  fireEvent.click(pagesMore as HTMLElement);
  expect(screen.queryByRole("list", { name: "Settings" })).toBeNull();
  fireEvent.change(field, { target: { value: "" } });
  expect(screen.queryByRole("list", { name: "Recent" })).toBeNull();
  const pages = within(screen.getByRole("list", { name: "Pages" }));
  expect(pages.getAllByRole("button").length).toBeGreaterThan(5);
});

test("Enter on a setting opens its closed section and focuses it", async () => {
  useUi.setPath("settings.closed", ["Shuffles/Fake section"]);
  renderApp();
  const field = find("cows fake");
  fireEvent.keyDown(field, { key: "Enter" });
  expect(dialog()).toBeNull();
  expect(useUi.getPath("settings.closed")).toEqual([]);
  await waitFor(() =>
    expect(document.activeElement?.id).toBe("setting-fake-cows"),
  );
  expect(useUi.getPath("find.recent")).toEqual([
    { group: "settings", key: "fake-cows" },
  ]);
});

test("Check for Updates stays open and says what it found", async () => {
  renderApp();
  // The App setting of that name lists first, under Settings.
  find("updates");
  const row = () =>
    within(screen.getByRole("list", { name: "Actions" })).getByRole("button");
  fireEvent.click(row());
  expect(checkNow).toHaveBeenCalled();
  await waitFor(() => expect(row().textContent).toContain("Up to date"));
  expect(dialog()).toBeTruthy();
});
