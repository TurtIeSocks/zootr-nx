import { createContext, use } from "react";
import type { GameOptions, Side } from "@/bindings";
import type { FullTest } from "@/lib/config";
import type { Loaded } from "@/lib/use-load";

/** The test setup as the form edits it: text where the setup has an
 * optional value. */
export interface TestForm {
  spawn: string;
  near: string;
  side: Side;
  /** Items to hand over, by actor. */
  give: string[];
  /** New-game flags, NAME=VALUE each. */
  defaults: string[];
  /** Quest steps, CHALLENGE=STEP each, in order. */
  progress: string[];
  /** What the dev panel waits on, NAME=VALUE each. */
  goals: string[];
  expect: string;
  /** Not edited here, only kept. */
  hour: FullTest["hour"];
}

/** The form for a saved test setup. */
export const toForm = (t: FullTest): TestForm => ({
  spawn: t.spawn ?? "",
  near: t.near ?? "",
  side: t.side ?? "front",
  give: t.give,
  defaults: t.defaults,
  progress: t.progress,
  goals: t.goals,
  expect: t.expect,
  hour: t.hour,
});

/** The test setup a form would save. */
export const fromForm = (f: TestForm): FullTest => ({
  spawn: f.spawn.trim() || null,
  give: f.give,
  defaults: f.defaults,
  progress: f.progress,
  near: f.near.trim() || null,
  side: f.side === "front" ? null : f.side,
  hour: f.hour,
  goals: f.goals,
  expect: f.expect.trim(),
});

/** The test setup form's state, shared by its fields. */
export interface TestFormState {
  form: TestForm;
  /** Lays `patch` over the form. */
  set: (patch: Partial<TestForm>) => void;
  /** What the dump offers, for the pickers. */
  options: Loaded<GameOptions>;
}

/** The test setup form around a field. */
export const TestFormContext = createContext<TestFormState | null>(null);

/** The test setup form around the caller. Throws outside one. */
export function useTestForm() {
  const state = use(TestFormContext);
  if (!state) throw new Error("useTestForm has to be used within TestPane");
  return state;
}
