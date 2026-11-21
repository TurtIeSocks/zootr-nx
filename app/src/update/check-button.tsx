import { RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import { TipButton } from "@/components/tip-button";
import { cn } from "@/lib/utils";
import { type Checked, checkNow } from "@/stores/update";

/** How long "Up to date" stays beside the button. */
const SHOWN = 4000;

/** What a check found, in words. A newer release needs none: the sidebar's
 * notice shows it. */
const SAID: Partial<Record<Checked, string>> = {
  current: "Up to date",
  failed: "Couldn't check",
};

/** Checks for a newer release now, even with checks off, then says what
 * it found for a moment. */
export function CheckButton() {
  const [state, setState] = useState<Checked | "checking" | null>(null);
  useEffect(() => {
    if (state === null || state === "checking") return;
    const timer = setTimeout(() => setState(null), SHOWN);
    return () => clearTimeout(timer);
  }, [state]);
  const said = state && state !== "checking" ? SAID[state] : undefined;
  return (
    <>
      {said && (
        <span role="status" className="text-ink-faint text-xs">
          {said}
        </span>
      )}
      <TipButton
        label="Check for updates"
        tip={state === "checking" ? "Checking…" : "Check for updates"}
        variant="ghost"
        disabled={state === "checking"}
        onClick={async () => {
          setState("checking");
          setState(await checkNow());
        }}
      >
        <RefreshCw className={cn(state === "checking" && "animate-spin")} />
      </TipButton>
    </>
  );
}
