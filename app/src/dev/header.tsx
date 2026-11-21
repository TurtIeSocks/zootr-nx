import { LoaderCircle, Pin, PinOff, Search } from "lucide-react";
import { commands } from "@/bindings";
import { Tip } from "@/components/tip";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/segmented";
import { area } from "@/dev/areas";
import { InfinitySwitches } from "@/dev/infinity-switches";
import { call } from "@/lib/api";
import { cn } from "@/lib/utils";
import { pin, send, useCanSend, useDev } from "@/stores/dev";

/** The window's head, always on screen: where Link is, his age, the
 * search and the pin, and the infinite switches. */
export function Header() {
  return (
    <header className="flex flex-col gap-3 bg-surface p-4 pb-3">
      <div className="flex items-start gap-2">
        <Where />
        <AgePicker />
        <Tip tip="⌘K">
          <Button
            size="icon-sm"
            variant="ghost"
            aria-label="Find anything"
            onClick={() => useDev.setPath("palette", true)}
          >
            <Search />
          </Button>
        </Tip>
        <PinButton />
      </div>
      <InfinitySwitches />
    </header>
  );
}

/** Whether the game is read: filled and pinging while it is, hollow when
 * it isn't. */
function Dot({ on }: { on: boolean }) {
  return (
    <span aria-hidden className="relative flex size-2 shrink-0">
      {on && (
        <span className="absolute inline-flex size-full animate-ping rounded-full bg-primary opacity-50" />
      )}
      <span
        className={cn(
          "relative inline-flex size-2 rounded-full",
          on ? "bg-primary" : "border-[1.5px] border-ink-faint",
        )}
      />
    </span>
  );
}

/** Where Link is: the last warp's spot while he's still on its map, else
 * the area, then the area and map id, and what holds the game up. Or why
 * the game can't be read. */
function Where() {
  const [{ state, error }] = useDev.usePath("game");
  const [warp] = useDev.usePath("lastWarp");
  const title = "flex items-center gap-2 font-semibold text-[15px] leading-5";

  if (!state)
    return (
      <div className="min-w-0 flex-1">
        <Tip focusable tip={error}>
          <p className={title}>
            <Dot on={false} />
            {error ? "Game not running" : "Looking for the game"}
          </p>
        </Tip>
        <p className="type-mono mt-0.5 text-ink-faint text-xs">—</p>
      </div>
    );
  const name = area(state.map);
  return (
    <div className="min-w-0 flex-1">
      <p className={title}>
        <Dot on />
        <span className="truncate">
          {warp?.map === state.map ? warp.spot : name}
        </span>
      </p>
      <p className="mt-0.5 flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-[12.5px] text-ink-faint">
        <span>{name}</span>
        <span aria-hidden>·</span>
        <span className="type-mono">{state.map}</span>
        {state.loading ? (
          <Badge variant="gold">
            <LoaderCircle className="animate-spin" />
            Loading
          </Badge>
        ) : (
          state.event && <Badge variant="gold">Event playing</Badge>
        )}
      </p>
    </div>
  );
}

const AGES = [
  { id: "child", label: "Child" },
  { id: "adult", label: "Adult" },
] as const;

/** Link's age as the game says it. Picking the other one swaps it. */
function AgePicker() {
  const [child] = useDev.usePath("game.state.child");
  const can = useCanSend();
  const age = child === undefined ? undefined : child ? "child" : "adult";

  return (
    <Segmented
      aria-label="Age"
      options={AGES}
      value={age}
      disabled={!can || age === undefined}
      onValueChange={(id) => {
        if (id === age) return;
        void send("age", `switch to ${id}`, () => call(commands.devAge()));
      }}
    />
  );
}

/** Keeps the window over the game, or lets it drop behind. */
function PinButton() {
  const [pinned] = useDev.usePath("pinned");
  return (
    <Button
      size="icon-sm"
      variant="ghost"
      className={cn(pinned && "text-primary-ink")}
      aria-label={pinned ? "Unpin from the top" : "Keep on top"}
      aria-pressed={pinned}
      onClick={pin}
    >
      {pinned ? <Pin /> : <PinOff />}
    </Button>
  );
}
