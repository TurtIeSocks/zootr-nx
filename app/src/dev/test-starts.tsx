import { useMemo } from "react";
import { commands, type Goal } from "@/bindings";
import { Tip } from "@/components/tip";
import { Button } from "@/components/ui/button";
import { DevSection } from "@/dev/section";
import { call } from "@/lib/api";
import { useRequest } from "@/lib/request";
import { cn } from "@/lib/utils";
import { useDev } from "@/stores/dev";

/** No goals or holds yet. Shared, so a selector's fallback stays the same
 * array between renders. */
const NONE: never[] = [];

/** The saved test setups, each applied live with a click. */
export function TestStarts() {
  return (
    <DevSection title="Test starts">
      <SetupList />
    </DevSection>
  );
}

/** Each saved setup, or how to save one. */
function SetupList() {
  const [names] = useDev.usePath("tests.names");

  if (names?.length === 0)
    return (
      <p className="text-ink-faint text-xs">
        Save a test setup in Settings to apply it here
      </p>
    );
  return (
    <ul className="flex flex-col gap-1">
      {names?.map((name) => {
        return <Setup key={name} name={name} />;
      })}
    </ul>
  );
}

interface SetupProps {
  /** The setup's name. It may hold a dot, so it's never put in a path. */
  name: string;
}

/** A setup's name and tick, opening to its goals and what to expect. */
function Setup({ name }: SetupProps) {
  const goals = useGoals(name);
  const expect = useDev((s) => s.tests.setups[name]?.expect);
  const before = useDev((s) => s.tests.holds[name]);
  const detail = goals.length > 0 || expect;

  return (
    <li className="flex items-start justify-between gap-2">
      <details className="min-w-0 flex-1 text-[13px]">
        <Tip tip={goals.map((g) => `${g.name}=${g.want}`).join("\n")}>
          <summary
            className={cn(
              "flex items-center gap-1.5 truncate",
              detail ? "cursor-pointer" : "list-none",
            )}
          >
            {goals.length > 0 && <Tick goals={goals} held={before} />}
            <span className="truncate">{name}</span>
          </summary>
        </Tip>
        {detail && (
          <ul className="mt-1 ml-4.5 flex flex-col gap-0.5 text-xs">
            {goals.map((g) => (
              <GoalLine
                key={g.name}
                goal={g}
                held={!!before?.includes(g.name)}
              />
            ))}
            {expect && <li className="text-ink-muted">{expect}</li>}
          </ul>
        )}
      </details>
      <ApplyButton name={name} />
    </li>
  );
}

/** A setup's goals as the game holds them, or as the config writes them
 * until the game answers. */
function useGoals(name: string) {
  const lines = useDev((s) => s.tests.setups[name]?.goals) ?? NONE;
  const read = useDev((s) => s.tests.goals[name]) ?? NONE;

  return useMemo(
    () => lines.map((l, i) => read[i] ?? unread(l)),
    [lines, read],
  );
}

interface ApplyButtonProps {
  /** The setup's name. */
  name: string;
}

/** Applies the setup now, keeping which of its goals held before. */
function ApplyButton({ name }: ApplyButtonProps) {
  const [ready] = useDev.usePath("ready");
  const request = useRequest();

  const apply = () =>
    request.run(async () => {
      const applied = await call(commands.devApply(name));
      useDev.setPath("tests.holds", (holds) => ({
        ...holds,
        [name]: applied.held,
      }));
      const early = applied.held.map((g) => `${g} held before Apply`);
      return [...applied.done, ...early].join("\n");
    });

  return (
    <Button
      size="sm"
      variant="secondary"
      disabled={!ready || request.busy}
      onClick={apply}
    >
      Apply now
    </Button>
  );
}

/** ✓ once every goal holds, amber when one held before Apply. */
function Tick({ goals, held }: { goals: Goal[]; held?: string[] }) {
  const met = goals.every((g) => g.met);
  const early = goals.some((g) => g.met && held?.includes(g.name));
  const failed = goals.some((g) => g.error);
  return (
    <span
      role="img"
      aria-label={met ? "Goals met" : "Goals open"}
      className={cn(
        "w-3 shrink-0 text-center",
        failed
          ? "text-danger-ink"
          : met
            ? early
              ? "text-gold-ink"
              : "text-primary-ink"
            : "text-ink-faint",
      )}
    >
      {met ? "✓" : "○"}
    </span>
  );
}

/** A goal, what the game holds for it, and whether it held before Apply. */
function GoalLine({ goal, held }: { goal: Goal; held: boolean }) {
  return (
    <li className={cn("type-mono", goal.error && "text-danger-ink")}>
      {goal.met ? "✓" : "○"} {goal.name}={goal.want}
      {(goal.error ?? goal.now) !== null && (
        <span className="text-ink-faint"> ({goal.error ?? goal.now})</span>
      )}
      {goal.met && held && (
        <span className="text-gold-ink"> held before Apply</span>
      )}
    </li>
  );
}

/** A goal line before the game has answered for it. */
function unread(line: string): Goal {
  const [name = "", ...want] = line.split("=");
  return {
    name: name.trim(),
    want: want.join("=").trim(),
    now: null,
    met: false,
    error: null,
  };
}
