import { RotateCcw } from "lucide-react";
import { type ReactNode, useState } from "react";
import { ChangeList } from "@/components/change-list";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import { nameOf } from "@/lib/format";
import {
  DEFAULTS_FROM,
  driftSince,
  takeSettings,
  useDriftBase,
  useDriftFrom,
  useSeeds,
} from "@/stores/seeds";

/** Props for {@link DriftReview}. */
interface DriftReviewProps {
  /** The button that opens it. */
  children: ReactNode;
  /** Where it opens: the bar's floats at the foot, so it opens above. */
  side?: "top" | "bottom";
}

/** Every difference by group, and a way back to what drift measures
 * from: Review, on the drift bar and on Play's pill. */
export function DriftReview({ children, side }: DriftReviewProps) {
  const [open, setOpen] = useState(false);
  const [drift] = useSeeds.usePath("drift");
  const base = useDriftBase();
  const from = useDriftFrom();
  const restore =
    from === DEFAULTS_FROM
      ? "Use the default settings"
      : `Use ${from}'s settings`;
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>{children}</PopoverTrigger>
      <PopoverContent align="end" side={side} className="w-[360px] p-3">
        <p className="type-overline pb-1">
          Changed {driftSince(from)} {(base && nameOf(base)) || from}
        </p>
        <ChangeList changes={drift} />
        <div className="-mx-3 my-2.5 h-px bg-line" />
        <div className="flex justify-end">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setOpen(false);
              takeSettings(base ?? { settings: {} });
            }}
          >
            <RotateCcw />
            {restore}
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
