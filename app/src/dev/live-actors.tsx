import { Eye, EyeOff, RefreshCw, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { MapGate } from "@/dev/actors";
import { DevSection } from "@/dev/section";
import {
  type HiddenChip,
  type LiveRow,
  readLive,
  showAll,
  showOne,
  toggle,
  useHiddenChips,
  useLiveReads,
  useLiveRows,
} from "@/dev/use-live-actors";
import { useRequest } from "@/lib/request";
import { cn } from "@/lib/utils";
import { useDev } from "@/stores/dev";

/** The live actors nearest Link, each one's model hidden or shown with a
 * click, and the ones hidden, hidden again after each load of their map. */
export function LiveActors() {
  return (
    <DevSection title="Live actors">
      <MapGate>
        <LiveReads />
        <HiddenStrip />
        <LiveControls />
        <LiveList />
      </MapGate>
    </DevSection>
  );
}

/** Keeps the list read. Shows nothing. */
function LiveReads() {
  useLiveReads();
  return null;
}

/** The actors hidden so far, each shown again with its ×, or all at once. */
function HiddenStrip() {
  const chips = useHiddenChips();
  const request = useRequest();
  const [ready] = useDev.usePath("ready");

  if (chips.length === 0) return null;
  return (
    <div className="mb-2 border-line border-b pb-2">
      <div className="flex items-center justify-between">
        <span className="text-ink-muted text-xs">Hidden ({chips.length})</span>
        <Button
          size="sm"
          variant="ghost"
          disabled={!ready || request.busy}
          onClick={() => request.run(showAll)}
        >
          Show all
        </Button>
      </div>
      <ul aria-label="Hidden actors" className="mt-1 flex flex-wrap gap-1">
        {chips.map((chip) => (
          <li key={chip.hidden.hash}>
            <Chip chip={chip} />
          </li>
        ))}
      </ul>
    </div>
  );
}

/** One hidden actor, dimmed while it isn't hidden in the game now. */
function Chip({ chip: { hidden, tag, live } }: { chip: HiddenChip }) {
  const request = useRequest();
  const [ready] = useDev.usePath("ready");

  return (
    <span
      className={cn(
        "inline-flex items-center gap-0.5 rounded-sm border border-line py-0.5 pl-2 text-[12px]",
        !live && "opacity-50",
      )}
    >
      {hidden.actor}
      {tag && <span className="text-ink-faint"> {tag}</span>}
      <Button
        size="icon-xs"
        variant="ghost"
        aria-label={`Show ${hidden.actor}${tag ? ` ${tag}` : ""}`}
        disabled={!ready || request.busy}
        onClick={() => request.run(() => showOne(hidden))}
      >
        <X />
      </Button>
    </span>
  );
}

/** The search, Refresh, and Follow. */
function LiveControls() {
  const [query, setQuery] = useDev.usePath("live.query");
  const [follow, setFollow] = useDev.usePath("live.follow");
  const request = useRequest();
  const [ready] = useDev.usePath("ready");

  return (
    <div className="flex items-center gap-2">
      <Input
        aria-label="Search live actors"
        placeholder="Search live actors"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      <Button
        size="sm"
        variant="secondary"
        disabled={!ready || request.busy}
        onClick={() => request.run(readLive)}
      >
        <RefreshCw />
        Refresh
      </Button>
      <Switch id="live-follow" checked={follow} onCheckedChange={setFollow} />
      <Label htmlFor="live-follow" className="text-xs">
        Follow
      </Label>
    </div>
  );
}

/** The live actors matching the search, nearest first at the last read,
 * or why there are none to show. */
function LiveList() {
  const live = useLiveRows();
  const [map] = useDev.usePath("game.state.map");

  return (
    <div className="mt-1.5">
      <div className="flex justify-between px-3 text-[11px] text-ink-faint">
        <span>Actor</span>
        <span>m (at refresh)</span>
      </div>
      <ul
        aria-label={`Live actors in ${map}`}
        className="max-h-72 overflow-auto rounded-md border border-line"
      >
        {live.status === "ok" ? (
          <LiveRows rows={live.rows} map={live.map} />
        ) : (
          <li className="px-2 py-1 text-ink-faint text-xs">
            {live.status === "error" ? live.error : "Reading the actors…"}
          </li>
        )}
      </ul>
    </div>
  );
}

function LiveRows({ rows, map }: { rows: LiveRow[]; map: string }) {
  if (rows.length === 0)
    return (
      <li className="px-2 py-1 text-ink-faint text-xs">No actors match</li>
    );
  return rows.map((row) => (
    <li key={row.near.hash}>
      {row.near.hidden === "game" ? (
        <GameHidden row={row} />
      ) : (
        <ActorToggle row={row} map={map} />
      )}
    </li>
  ));
}

/** A row's name, tag and distance, after its leading mark. */
function RowText({
  row: { near, tag },
  note,
}: {
  row: LiveRow;
  note?: string;
}) {
  return (
    <>
      <span className="truncate">{near.actor}</span>
      {tag && <span className="truncate text-ink-faint">{tag}</span>}
      <span className="ml-auto shrink-0 text-ink-faint tabular-nums">
        {note ?? `${near.distance?.toFixed(1) ?? "?"} m`}
      </span>
    </>
  );
}

const ROW =
  "flex w-full items-center gap-2 px-3 py-0.5 text-left text-[13px] [&_svg]:size-3.5 [&_svg]:shrink-0";

/** Hides or shows one actor's model. */
function ActorToggle({ row, map }: { row: LiveRow; map: string }) {
  const request = useRequest();
  const [ready] = useDev.usePath("ready");
  const hidden = row.near.hidden === "ours";

  return (
    <button
      type="button"
      aria-pressed={hidden}
      disabled={!ready || request.busy}
      onClick={() => request.run(() => toggle(row.near, map))}
      className={cn(
        ROW,
        "cursor-pointer enabled:hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-45",
        hidden && "text-ink-muted",
      )}
    >
      {hidden ? <EyeOff aria-hidden /> : <Eye aria-hidden />}
      <RowText row={row} />
    </button>
  );
}

/** An actor the game keeps hidden: shown, not toggled, since a show never
 * undoes the game's own hide. */
function GameHidden({ row }: { row: LiveRow }) {
  return (
    <div className={cn(ROW, "text-ink-faint")}>
      <EyeOff aria-hidden className="opacity-50" />
      <RowText row={row} note="hidden by game" />
    </div>
  );
}
