import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import type { StepEvent } from "@/bindings";
import { GenerateSteps } from "@/play/generate-steps";
import { type Steps, stepped, useSeeds } from "@/stores/seeds";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/** A run that read the dump and is reading checks. */
const events: StepEvent[] = [
  { kind: "began", step: "dump" },
  { kind: "detail", text: "cached" },
  { kind: "ended", step: "dump", detail: "cached", secs: 0.4 },
  { kind: "began", step: "checks" },
];
const running = events.reduce(stepped, { rows: [], end: null, secs: 0 });

/** Renders the step list for `steps`, while `generating` or after. */
function show(steps: Steps, generating: boolean) {
  useSeeds.setPath("steps", steps);
  useSeeds.setPath("generating", generating);
  render(<GenerateSteps />);
}

const label = (name: string) => screen.queryByLabelText(name);

test("a running step spins beside the finished ones", () => {
  show(running, true);
  expect(screen.getByText("Reading the dump")).toBeTruthy();
  expect(screen.getByText("cached")).toBeTruthy();
  expect(screen.getByText("0.4 s")).toBeTruthy();
  expect(screen.getByText("Reading checks")).toBeTruthy();
  expect(label("Running")).toBeTruthy();
  expect(screen.queryByText("Building logic")).toBeNull();
  expect(screen.queryByText("Copy Log")).toBeNull();
});

test("a finished run is one line until Details opens its steps", () => {
  show({ ...running, end: { kind: "done" }, secs: 5.3 }, false);
  expect(screen.getByText(/Generated in 5.3 s/)).toBeTruthy();
  expect(screen.queryByText("Reading the dump")).toBeNull();
  expect(screen.getByText("Copy Log")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Details" }));
  expect(screen.getByText("Reading the dump")).toBeTruthy();
});

test("a failed run marks the step it failed in", () => {
  const error = { kind: "failed", error: "broke" } as const;
  show({ ...running, end: error }, false);
  expect(label("Failed")).toBeTruthy();
  expect(label("Running")).toBeNull();
  expect(screen.getByText("Copy Log")).toBeTruthy();
});

test("a stopped run says what the targets hold", () => {
  const end = { kind: "stopped", took_off: false } as const;
  const stopped = "Stopped: the targets are as they were";
  show({ ...running, end, stopped }, false);
  expect(screen.getByText(stopped)).toBeTruthy();
  expect(screen.getByText("Copy Log")).toBeTruthy();
});

test("a finished rebuild says so and has no log to copy", () => {
  useSeeds.setPath("rebuilt", "FakeBuild");
  const rows = [{ kind: "began", step: "rebuilding" }] as StepEvent[];
  const rebuilt = rows.reduce(stepped, { rows: [], end: null, secs: 0 });
  show({ ...rebuilt, end: { kind: "done" }, secs: 840 }, false);
  expect(screen.getByText(/Rebuilt in 840.0 s/)).toBeTruthy();
  expect(screen.queryByText("Copy Log")).toBeNull();
  useSeeds.setPath("rebuilt", null);
});
