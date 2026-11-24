import { Check, CircleAlert, CircleStop, Loader2 } from "lucide-react";
import { useEffect, useState } from "react";
import { InlineError } from "@/components/callout";
import { linkClass } from "@/components/styles";
import { message } from "@/lib/api";
import { useCopied } from "@/lib/use-copied";
import {
  generateLog,
  type StepRow,
  type Steps,
  stepName,
  useSeeds,
} from "@/stores/seeds";

/** How long a step runs before its hint shows, in milliseconds. */
const HINT_AFTER = 10_000;

/** Seconds as the step list shows them. */
const seconds = (secs: number) => `${secs.toFixed(1)} s`;

/** Why a step that runs long may be slow, where something helps. */
function hint(row: StepRow) {
  if (row.step === "dump" && row.detail.startsWith("extracting")) {
    return "A first run extracts about 230 MB of the dump once. Later seeds reuse it.";
  }
}

/** The running or last generate run's steps, in the folder that started
 * it: Archipelago for a slot (`ap`), Standard for the rest. Why the last
 * job failed shows right under them. */
export function GenerateSteps({ ap = false }: { ap?: boolean }) {
  const [error] = useSeeds.usePath("error");
  return (
    <>
      <StepList ap={ap} />
      <InlineError error={error} />
    </>
  );
}

/** The run's steps, or nothing before one ran in this folder. */
function StepList({ ap }: { ap: boolean }) {
  const [steps] = useSeeds.usePath("steps");
  const [generating] = useSeeds.usePath("generating");
  // A rebuild's run has no log of its own.
  const [rebuilt] = useSeeds.usePath("rebuilt");
  const [open, setOpen] = useState(false);
  // A run that failed before its first step has nothing to show.
  if (!steps || (!steps.rows.length && !generating)) return null;
  if (!!steps.ap !== ap) return null;
  if (steps.end?.kind === "done" && !open) {
    return (
      <div className="flex items-center gap-2 text-[13px] text-ink-muted">
        <Check className="size-3.5 text-primary-ink" />
        {rebuilt ? "Rebuilt" : "Generated"} in {seconds(steps.secs)}
        <button
          type="button"
          className={linkClass}
          onClick={() => setOpen(true)}
        >
          Details
        </button>
        {!rebuilt && <CopyLog />}
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-1 text-[13px]">
      <ul aria-label="Steps" className="flex flex-col gap-1">
        {steps.rows.map((row, i) => (
          <StepLine
            key={`${row.step}-${row.started}`}
            row={row}
            failed={steps.end?.kind === "failed" && i === steps.rows.length - 1}
            running={generating && row.secs === null}
          />
        ))}
      </ul>
      {!generating && <Ending steps={steps} log={!rebuilt} />}
    </div>
  );
}

/** Props for {@link StepLine}. */
interface StepLineProps {
  row: StepRow;
  /** The run failed in this step. */
  failed: boolean;
  /** It's the step running now. */
  running: boolean;
}

/** How a step stands, and the mark each state shows. */
const MARKS = {
  failed: { Icon: CircleAlert, label: "Failed", className: "text-danger-ink" },
  running: {
    Icon: Loader2,
    label: "Running",
    className: "animate-spin text-ink-muted",
  },
  stopped: { Icon: CircleStop, label: "Stopped", className: "text-ink-muted" },
  done: { Icon: Check, label: "Done", className: "text-primary-ink" },
} as const;

/** One step: a check and its time once ended, a spinner and a live timer
 * while it runs. A step the run failed or stopped in shows no time, since
 * its clock stopped with the run. */
function StepLine({ row, failed, running }: StepLineProps) {
  const now = useNow(running);
  const state = failed
    ? "failed"
    : running
      ? "running"
      : row.secs === null
        ? "stopped"
        : "done";
  const { Icon, label, className } = MARKS[state];
  const secs = running ? (now - row.started) / 1000 : row.secs;
  const long = running && now - row.started > HINT_AFTER;
  return (
    <li className="flex flex-col">
      <span className="flex items-center gap-2">
        <Icon aria-label={label} className={`size-3.5 ${className}`} />
        <span>{stepName(row.step)}</span>
        {row.detail && <span className="text-ink-muted">{row.detail}</span>}
        {secs !== null && (
          <span className="ml-auto font-mono text-ink-muted">
            {seconds(secs)}
          </span>
        )}
      </span>
      {long && hint(row) && (
        <span className="pl-5.5 text-ink-muted">{hint(row)}</span>
      )}
    </li>
  );
}

/** `Date.now()`, kept current while `ticking`. */
function useNow(ticking: boolean) {
  const [now, setNow] = useState(Date.now);
  useEffect(() => {
    if (!ticking) return;
    const timer = setInterval(() => setNow(Date.now()), 100);
    return () => clearInterval(timer);
  }, [ticking]);
  return now;
}

/** How a run that didn't finish ended, and its log when it has one. */
function Ending({ steps, log }: { steps: Steps; log: boolean }) {
  const stopped = steps.end?.kind === "stopped";
  return (
    <div className="flex items-center gap-2 text-ink-muted">
      {stopped && <CircleStop className="size-3.5" />}
      {stopped && <span>{steps.stopped ?? "Stopped"}</span>}
      {log && <CopyLog />}
    </div>
  );
}

/** Copies the run's log, for a bug report. */
function CopyLog() {
  const { copied, flash } = useCopied();
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(await generateLog());
      flash("log");
    } catch (e) {
      useSeeds.setPath("error", message(e));
    }
  };
  return (
    <button type="button" className={`${linkClass} ml-auto`} onClick={copy}>
      {copied ? "Copied" : "Copy Log"}
    </button>
  );
}
