import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test } from "vitest";
import { useUpdate } from "@/stores/update";
import { Version } from "@/update/version";

afterEach(cleanup);

const build = (commit: string, release: boolean) => {
  useUpdate.setState({ current: "1.2.3", commit, release });
  render(<Version />);
};

/** The tip's text once the version is focused, or null without one. */
const tip = async () => {
  fireEvent.focus(screen.getByText(/./));
  return (
    (await screen.findAllByRole("tooltip").catch(() => []))[0]?.textContent ??
    null
  );
};

test("a release shows its version, with no tip", async () => {
  build("abc1234", true);
  expect(screen.getByText("v1.2.3")).toBeTruthy();
  expect(await tip()).toBeNull();
});

test("a source build shows its commit, with the version in the tip", async () => {
  build("abc1234*", false);
  expect(screen.getByText("abc1234*")).toBeTruthy();
  expect(await tip()).toBe("v1.2.3");
});

test("a source build without git shows its version", () => {
  build("", false);
  expect(screen.getByText("v1.2.3")).toBeTruthy();
});
