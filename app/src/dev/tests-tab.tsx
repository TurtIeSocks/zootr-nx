import { Check, Circle, CircleAlert } from "lucide-react";
import { useMemo } from "react";
import type { Goal } from "@/bindings";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { apply } from "@/dev/requests";
import { cn } from "@/lib/utils";
import { useCanSend, useDev } from "@/stores/dev";

/** No goals or holds yet. Shared, so a selector's fallback stays the same
 * array between renders. */
const NONE: never[] = [];

/** The saved test setups in one card, each applied live with a click. */
export function TestsTab() {
  const [names] = useDev.usePath("tests.names");

  return (
    <div className="flex flex-col gap-2.5">
      {!!names?.length && (
        <ul
          aria-label="Test setups"
          className="divide-y divide-line rounded-lg border border-line bg-surface shadow-xs"
        >
          {names.map((name) => (
            <Setup key={name} name={name} />
          ))}
        </ul>
      )}
      {names?.length === 0 && (
        <p className="py-2 text-center text-[13px] text-ink-faint">
          Save a test setup in Settings → Test Setup to list it here
        </p>
      )}
    </div>
  );
}

interface SetupProps {
  /** The setup's name. It may hold a dot, so it's never put in a path. */
  name: string;
}

/** A setup: its met goals of all, name and Apply Now, then its goals as
 * the game holds them and what to expect. */
function Setup({ name }: SetupProps) {
  const goals = useGoals(name);
  const expect = useDev((s) => s.tests.setups[name]?.expect);
  const before = useDev((s) => s.tests.holds[name]);
  const can = useCanSend();

  return (
    <li className="p-3">
      <div className="flex items-center gap-2">
        <Tally goals={goals} />
        <span className="min-w-0 flex-1 truncate font-medium text-[13.5px]">
          {name}
        </span>
        <Button
          size="sm"
          variant="secondary"
          disabled={!can}
          onClick={() => void apply(name)}
        >
          Apply Now
        </Button>
      </div>
      {(goals.length > 0 || expect) && (
        <ul className="mt-2 flex flex-col gap-1 text-xs">
          {goals.map((g) => (
            <GoalLine key={g.name} goal={g} held={!!before?.includes(g.name)} />
          ))}
          {expect && <li className="mt-0.5 text-ink-muted">{expect}</li>}
        </ul>
      )}
      {goals.length === 0 && !expect && (
        <p className="mt-1.5 text-ink-faint text-xs">
          No goals. Applies the spawn and items only
        </p>
      )}
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

/** Met goals of all, green once every goal holds. A dash with none. */
function Tally({ goals }: { goals: Goal[] }) {
  const met = goals.filter((g) => g.met).length;
  const all = goals.length > 0 && met === goals.length;
  return (
    <Badge
      shape="square"
      variant={all ? "green" : "default"}
      role="img"
      aria-label={
        goals.length === 0 ? "No goals" : all ? "Goals met" : "Goals open"
      }
      className="tabular-nums"
    >
      {goals.length === 0 ? "—" : `${met}/${goals.length}`}
    </Badge>
  );
}

/** A goal, its mark, what the game holds for it, and whether it held
 * before Apply. */
function GoalLine({ goal, held }: { goal: Goal; held: boolean }) {
  const early = goal.met && held;
  const Mark = goal.error ? CircleAlert : goal.met ? Check : Circle;
  return (
    <li
      className={cn(
        "type-mono flex items-start gap-1.5",
        goal.error && "text-danger-ink",
      )}
    >
      <Mark
        aria-hidden
        className={cn(
          "mt-px size-3.5 shrink-0",
          goal.error
            ? "text-danger-ink"
            : goal.met
              ? early
                ? "text-gold"
                : "text-primary-ink"
              : "text-ink-faint",
        )}
      />
      <span className="min-w-0 break-all">
        {goal.name}={goal.want}
        {(goal.error ?? goal.now) !== null && !goal.met && (
          <span className="text-ink-faint">
            {" "}
            ({goal.error ?? `now ${goal.now}`})
          </span>
        )}
        {early && <span className="text-ink-faint"> (held before Apply)</span>}
      </span>
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
