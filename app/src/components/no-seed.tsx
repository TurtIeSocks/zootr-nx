import { Sparkles } from "lucide-react";
import { Button } from "@/components/ui/button";
import { go } from "@/stores/shell";

/** What a view of the installed seed shows while none is installed: where
 * one is made. */
export function NoSeed() {
  return (
    <div className="flex flex-1 flex-col items-center justify-center gap-2 px-6 py-12 text-center">
      <Sparkles aria-hidden className="size-7 text-ink-muted" />
      <h2 className="font-semibold text-[15px]">No seed is installed</h2>
      <p className="text-[13px] text-ink-muted">
        Generate and install one on the Play tab first.
      </p>
      <Button
        variant="secondary"
        className="mt-1"
        onClick={() => go({ tab: "Play" })}
      >
        Go to Play
      </Button>
    </div>
  );
}
