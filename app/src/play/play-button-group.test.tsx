import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { full } from "@/lib/config";
import { RequestProvider } from "@/lib/request-provider";
import { PlayButtonGroup } from "@/play/play-button-group";
import { useConfig } from "@/stores/config";

afterEach(cleanup);

/** Renders the group with targets named `names`. */
function show(...names: string[]) {
  useConfig.setPath(
    "config",
    full({
      targets: names.map((name) => ({ name, app: "/a", mod_dir: "/m" })),
    }),
  );
  render(
    <RequestProvider>
      <PlayButtonGroup />
    </RequestProvider>,
  );
}

const menu = () => screen.queryByRole("button", { name: "Pick a target" });

test("with no target, only a disabled Play shows", () => {
  show();
  expect(
    (screen.getByRole("button", { name: /^Play/ }) as HTMLButtonElement)
      .disabled,
  ).toBe(true);
  expect(menu()).toBeNull();
});

test("with one target, only Play shows", () => {
  show("FakeOne");
  expect(screen.getByRole("button", { name: "Play FakeOne" })).toBeTruthy();
  expect(menu()).toBeNull();
});

test("with two targets, the menu picks between them", () => {
  show("FakeOne", "FakeTwo");
  expect(screen.getByRole("button", { name: "Play FakeOne" })).toBeTruthy();
  expect(menu()).toBeTruthy();
});
