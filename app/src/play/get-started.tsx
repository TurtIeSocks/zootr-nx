import { Check, X } from "lucide-react";
import { Card } from "@/components/card";
import { TipButton } from "@/components/tip-button";
import { cn } from "@/lib/utils";
import { useConfig } from "@/stores/config";
import { useNoDump, useSeeds } from "@/stores/seeds";
import { openDumpField, openSettings } from "@/stores/shell";
import { useUi } from "@/stores/ui";

/** One step: ticked once done, a link while it has a page to open. */
interface Step {
  label: string;
  done: boolean;
  open?: () => void;
}

/** The steps from a fresh install to playing a seed, until the first seed
 * lands in history (uninstalling it later doesn't bring them back) or the
 * card is dismissed. */
export function GetStarted() {
  const started = useSeeds((s) => s.history.entries.length > 0);
  const [dismissed, setDismissed] = useUi.usePath("getStarted.dismissed");
  const dump = !useNoDump();
  const target = useConfig((s) => s.config.targets.length > 0);
  if (started || dismissed) return null;
  const steps: Step[] = [
    { label: "Choose your game dump", done: dump, open: openDumpField },
    {
      label: "Add where you play",
      done: target,
      open: () => openSettings("targets"),
    },
    { label: "Generate & Install a seed", done: false },
    { label: "Press Play and start a new file", done: false },
  ];
  return (
    <Card
      title="Get Started"
      action={
        <TipButton
          label="Dismiss"
          tip="Dismiss"
          variant="ghost"
          onClick={() => setDismissed(true)}
        >
          <X />
        </TipButton>
      }
    >
      <ol className="flex flex-col gap-2 px-4 pt-1 pb-4 text-[13px]">
        {steps.map((s, i) => (
          <li key={s.label} className="flex items-center gap-2.5">
            <span
              className={cn(
                "flex size-5 shrink-0 items-center justify-center rounded-full font-semibold text-[11px]",
                s.done
                  ? "bg-primary text-primary-foreground"
                  : "bg-surface-sunken text-ink-muted",
              )}
            >
              {s.done ? <Check className="size-3" aria-label="done" /> : i + 1}
            </span>
            {s.open && !s.done ? (
              <button
                type="button"
                className="cursor-pointer text-ink underline underline-offset-2"
                onClick={s.open}
              >
                {s.label}
              </button>
            ) : (
              <span className={cn(s.done ? "text-ink-muted" : "text-ink")}>
                {s.label}
              </span>
            )}
          </li>
        ))}
      </ol>
    </Card>
  );
}
