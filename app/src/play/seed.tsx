import { FlaskConical, Globe, type LucideIcon, Pin } from "lucide-react";
import type { Summary } from "@/bindings";
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
    <Tooltip>
      <TooltipTrigger asChild tabIndex={0}>
        <Badge variant={variant}>
          <Icon />
          {label}
        </Badge>
      </TooltipTrigger>
      <TooltipContent>{tip}</TooltipContent>
    </Tooltip>
  );
}

/** Marks a seed built with a plando or a test setup. */
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
