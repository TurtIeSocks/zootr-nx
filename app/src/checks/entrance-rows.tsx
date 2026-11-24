import { Circle, CircleCheck, Ellipsis } from "lucide-react";
import type { DoorRow } from "@/bindings";
import { dungeonName } from "@/checks/doors";
import { AreaHeading } from "@/checks/table";
import { RequestError } from "@/components/request-error";
import { cellClass } from "@/components/styles";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { cn } from "@/lib/utils";
import { pick, useChecks } from "@/stores/checks";

/** Props for the components that show one door. */
interface DoorProps {
  door: DoorRow;
  doors: DoorRow[];
}

/** Guess the dungeon behind a door, or clear the guess. */
function PickMenu({ door, doors }: DoorProps) {
  const picking = useRequest();
  const choose = (dungeon: string | null) =>
    picking.run(() => pick(door.id, dungeon));
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="icon-sm" variant="ghost" aria-label={`Pick ${door.name}`}>
          <Ellipsis />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        {doors.map((d) => (
          <DropdownMenuItem key={d.id} onSelect={() => choose(d.id)}>
            {d.vanilla}
          </DropdownMenuItem>
        ))}
        {door.pick && (
          <DropdownMenuItem onSelect={() => choose(null)}>
            Clear Pick
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** Where a door leads: found, guessed, or `?`. */
function Behind({ door, doors }: DoorProps) {
  if (door.found) return <>{dungeonName(doors, door.found)}</>;
  if (door.pick)
    return (
      <span className="text-ink-muted italic">
        {dungeonName(doors, door.pick)}
      </span>
    );
  return <span className="text-ink-faint">?</span>;
}

/** A door's row, with its own request for picking. A found door can't be
 * picked. */
function EntranceRow({ door, doors }: DoorProps) {
  return (
    <RequestProvider>
      <tr className="transition-colors duration-(--duration-fast) hover:bg-surface-hover">
        <td className={cn(cellClass, "w-10 pr-0")}>
          {door.found ? (
            <CircleCheck className="size-4 text-primary" aria-label="found" />
          ) : (
            <Circle className="size-4 text-control-border" aria-hidden />
          )}
        </td>
        <td className={cellClass}>{door.name}</td>
        <td className={cellClass}>
          <Behind door={door} doors={doors} />
        </td>
        <td className={cn(cellClass, "text-ink-muted")}>
          <RequestError />
        </td>
        <td className={cn(cellClass, "w-12 text-right")}>
          {!door.found && <PickMenu door={door} doors={doors} />}
        </td>
      </tr>
    </RequestProvider>
  );
}

/** The seed's shuffled doors, above the checks. Nothing when none shuffle. */
export function EntranceRows() {
  const [doors] = useChecks.usePath("doors");
  if (doors.length === 0) return null;
  const found = doors.filter((d) => d.found).length;
  return (
    <tbody className="group/area">
      <AreaHeading name="Entrances" done={found} total={doors.length} />
      {doors.map((d) => (
        <EntranceRow key={d.id} door={d} doors={doors} />
      ))}
    </tbody>
  );
}
