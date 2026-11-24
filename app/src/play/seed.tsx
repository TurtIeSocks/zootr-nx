import {
  FlaskConical,
  Globe,
  type LucideIcon,
  Pin,
  TriangleAlert,
} from "lucide-react";
import type { Summary } from "@/bindings";
import { Tip } from "@/components/tip";
import { Badge } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { nameOf } from "@/lib/format";
import { cn } from "@/lib/utils";

/** Props for {@link ExtrasBadges} and {@link Name}. */
interface SeedProps {
  entry: Summary;
  className?: string;
}

/** Props for {@link TipBadge}. */
interface TipBadgeProps {
  icon: LucideIcon;
  label: string;
  tip: string;
  variant?: "info" | "ap";
}

/** A small badge that says more on hover. */
function TipBadge({ icon: Icon, label, tip, variant = "info" }: TipBadgeProps) {
  return (
    <Tip focusable tip={tip}>
      <Badge variant={variant}>
        <Icon />
        {label}
      </Badge>
    </Tip>
  );
}

/** Marks an Archipelago slot, a plando, a test setup or a seed for the
 * base game. */
export function ExtrasBadges({ entry }: SeedProps) {
  return (
    <>
      {entry.ap_slot && (
        <TipBadge
          icon={Globe}
          label="Archipelago"
          tip="Your slot of an Archipelago multiworld"
          variant="ap"
        />
      )}
      {entry.plando && (
        <TipBadge
          icon={Pin}
          label="Plando"
          tip="Built with a plando: items pinned to checks"
        />
      )}
      {entry.test && (
        <TipBadge
          icon={FlaskConical}
          label="Test"
          tip="Built with a test setup: a spawn, items or new-game flags"
        />
      )}
      {entry.release === "1.0.0" && (
        <TipBadge
          icon={TriangleAlert}
          label="1.0.0"
          tip="Base game (1.0.0) support is experimental and not verified in game"
        />
      )}
    </>
  );
}

/** A seed's name, showing the whole name and its note on hover. */
export function Name({ entry, className }: SeedProps) {
  const name = nameOf(entry);
  const { note } = entry.label;
  if (!name) return null;
  return (
    <Tooltip>
      <TooltipTrigger asChild tabIndex={0}>
        <p className={cn("truncate", className)}>{name}</p>
      </TooltipTrigger>
      <TooltipContent side="bottom" align="start">
        <p className="font-medium">{name}</p>
        {note && (
          <p className="mt-0.5 whitespace-pre-line opacity-70">{note}</p>
        )}
      </TooltipContent>
    </Tooltip>
  );
}
