import { Dices, Loader2 } from "lucide-react";
import { ChangedDot } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { generate, useSeeds } from "@/stores/seeds";

/** Floats at the foot of a settings page while the settings differ from the
 * installed seed's, with what differs and a way to make a seed from them. */
export function DriftBar() {
  const [changes] = useSeeds.usePath("drift");
  const [blocked] = useSeeds.usePath("blocked");
  const [busy] = useSeeds.usePath("busy");
  const n = changes.length;
  const generateButton = (
    <Button
      aria-disabled={!!blocked || undefined}
      disabled={busy}
      onClick={blocked ? undefined : () => generate(null)}
    >
      {busy ? <Loader2 className="animate-spin" /> : <Dices />}
      Generate & install
    </Button>
  );
  return (
    <div className="sticky bottom-5 z-10 mx-auto flex w-full max-w-160 animate-rise items-center gap-3 rounded-xl border border-line bg-surface-raised py-2 pr-2 pl-4 text-[13px] shadow-lg">
      <ChangedDot className="size-2" />
      <p className="min-w-0 flex-1">
        <b className="font-semibold">
          {n} setting{n === 1 ? "" : "s"}
        </b>{" "}
        {n === 1 ? "differs" : "differ"} from the installed seed
      </p>
      <Popover>
        <PopoverTrigger asChild>
          <Button variant="ghost">Review</Button>
        </PopoverTrigger>
        <PopoverContent side="top" className="text-[13px]">
          <p className="mb-1 font-medium">Changed since the installed seed</p>
          <ul className="flex flex-col gap-0.5 text-ink-muted">
            {changes.map((c) => (
              <li key={c}>{c}</li>
            ))}
          </ul>
        </PopoverContent>
      </Popover>
      {blocked ? (
        <Tooltip>
          <TooltipTrigger asChild>{generateButton}</TooltipTrigger>
          <TooltipContent>{blocked}</TooltipContent>
        </Tooltip>
      ) : (
        generateButton
      )}
    </div>
  );
}
