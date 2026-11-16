import { Badge, ChangedDot } from "@/components/ui/badge";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useDriftFrom } from "@/stores/seeds";

/** Props for {@link SettingsChanged}. */
interface SettingsChangedProps {
  /** The changed settings, one line each. */
  changes: string[];
}

/** Flags settings that differ from the installed seed's (the defaults with
 * none installed), listing them on hover. */
export function SettingsChanged({ changes }: SettingsChangedProps) {
  const from = useDriftFrom();
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Badge asChild variant="gold" className="cursor-default">
          <button type="button">
            <ChangedDot /> Not installed yet
          </button>
        </Badge>
      </TooltipTrigger>
      <TooltipContent side="bottom" align="start">
        <p className="mb-1 font-medium">Changed from {from}</p>
        <ul>
          {changes.map((c) => (
            <li key={c}>{c}</li>
          ))}
        </ul>
      </TooltipContent>
    </Tooltip>
  );
}
