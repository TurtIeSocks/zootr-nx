import {
  CircleCheck,
  Download,
  Loader2,
  Pencil,
  ScrollText,
  Star,
  Trash2,
} from "lucide-react";
import { memo, useCallback, useDeferredValue, useRef, useState } from "react";
import type { Summary } from "@/bindings";
import { CardHeader } from "@/components/card";
import { CopySeed, SeedIcons } from "@/components/seed-icons";
import { divided } from "@/components/styles";
import { TipButton } from "@/components/tip-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/segmented";
import { nameOf, when } from "@/lib/format";
import { cn } from "@/lib/utils";
import { HISTORY_SHOWS, PAGE } from "@/play/history-filter";
import { NONE, type Selection, toggled } from "@/play/history-select";
import { SelectBar } from "@/play/history-select-bar";
import {
  ConfirmButton,
  ConfirmText,
  DiffersIn,
  ListRow,
  UseSettings,
} from "@/play/list-row";
import { ExtrasBadges, Name } from "@/play/seed";
import { rowId } from "@/shell/find/reveal";
import {
  clearHistory,
  deleteSeeds,
  install,
  starSeeds,
  takeDraft,
  useDiffCount,
  useSeeds,
} from "@/stores/seeds";
import { openLabel, openSpoiler, useShell } from "@/stores/shell";
import { type HistoryFilter, useUi } from "@/stores/ui";

/** Deletes every seed but the installed and starred ones, on a second
 * click. */
function ClearHistory() {
  return (
    <ConfirmText
      label="Clear"
      tip="Delete every seed except installed and starred ones"
      confirm="Really clear?"
      onConfirm={clearHistory}
    />
  );
}

/** Props for the row actions that act on one seed. */
interface SeedActionProps {
  /** The seed's id. */
  id: number;
}

/** Installs the seed, unless something blocks it or a job runs. */
function InstallSeed({ id }: SeedActionProps) {
  const [blocked] = useSeeds.usePath("blocked");
  const [busy] = useSeeds.usePath("busy");
  return (
    <TipButton
      label="Install"
      tip={blocked || "Install this seed to every target"}
      size="sm"
      disabled={busy || !!blocked}
      onClick={() => install(id)}
    >
      <Download />
      Install
    </TipButton>
  );
}

/** Props for {@link DeleteSeed}. */
interface DeleteSeedProps extends SeedActionProps {
  /** The installed seed can't be deleted. */
  installed: boolean;
}

/** Deletes the seed on a second click, while no job runs. */
function DeleteSeed({ id, installed }: DeleteSeedProps) {
  const [busy] = useSeeds.usePath("busy");
  return (
    <ConfirmButton
      label="Delete seed"
      tip={
        installed ? "The installed seed can't be deleted" : "Delete this seed"
      }
      confirm="Really delete?"
      icon={<Trash2 />}
      disabled={busy || installed}
      onConfirm={() => deleteSeeds([id])}
    />
  );
}

/** Props for {@link StarSeed}. */
interface StarSeedProps extends SeedActionProps {
  starred: boolean;
}

/** Stars the seed so trimming and Clear pass it by, or unstars it. */
function StarSeed({ id, starred }: StarSeedProps) {
  const [busy] = useSeeds.usePath("busy");
  return (
    <TipButton
      label={starred ? "Unstar seed" : "Star seed"}
      tip={
        starred
          ? "Unstar: old seeds and Clear can delete it again"
          : "Star: old seeds and Clear leave it alone"
      }
      variant="ghost"
      pressed={starred}
      disabled={busy}
      onClick={() => starSeeds([id], !starred)}
    >
      <Star className={cn(starred && STARRED)} />
    </TipButton>
  );
}

/** A star's look once starred, as the dev panel's. */
const STARRED = "fill-gold text-gold";

/** Props for {@link InstallMark}. */
interface InstallMarkProps {
  /** Another seed is being installed over this one. */
  replaced?: boolean;
  /** This seed is being installed. */
  installing?: boolean;
}

/** Stays visible where Install would be: on the installed row, dark while
 * another seed replaces it, and on the row being installed, spinning. */
function InstallMark({ replaced, installing }: InstallMarkProps) {
  return (
    <Badge
      asChild
      variant={installing ? "info" : replaced ? "default" : "green"}
      className="cursor-default"
    >
      <button type="button" aria-disabled="true">
        {installing ? <Loader2 className="animate-spin" /> : <CircleCheck />}
        {installing ? "Installing" : "Installed"}
      </button>
    </Badge>
  );
}

const FILTERS = [
  { id: "all", label: "All" },
  { id: "standard", label: "Standard" },
  { id: "archipelago", label: "Archipelago" },
  { id: "starred", label: "Starred" },
] as const satisfies readonly { id: HistoryFilter; label: string }[];

/** What a filter that keeps no seed says. */
const EMPTY: Record<HistoryFilter, string> = {
  all: "No seeds yet.",
  standard: "No standard seeds yet.",
  archipelago: "No Archipelago slots yet.",
  starred: "No starred seeds yet.",
};

/** Rows the tab draws on the click itself. The rest of the page follows in
 * the background, where it can't hold the click up. */
const FIRST = 20;

/** "N seeds", and "· M starred" when any are. */
function counted(shown: Summary[]) {
  const n = shown.length;
  const starred = shown.filter((e) => e.starred).length;
  return `${n} seed${n === 1 ? "" : "s"}${starred ? ` · ${starred} starred` : ""}`;
}

/** The History folder: every generated seed, newest first, with its
 * actions a hover away. Draws a page of rows at a time. Select mode picks
 * many seeds to star, unstar or delete at once. */
export function HistoryList() {
  const [entries] = useSeeds.usePath("history.entries");
  const [filter, setFilter] = useUi.usePath("play.history");
  const [drawn = PAGE, setDrawn] = useShell.usePath("historyDrawn");
  const drawnNow = useDeferredValue(drawn, Math.min(FIRST, drawn));
  const [selecting, setSelecting] = useState(false);
  const [sel, setSel] = useState<Selection>(NONE);
  const shown = entries.filter(HISTORY_SHOWS[filter]);
  const ids = useRef<number[]>([]);
  ids.current = shown.map((e) => e.id);
  const toggle = useCallback(
    (id: number, range: boolean) =>
      setSel((s) => toggled(s, id, ids.current, range)),
    [],
  );
  const done = () => {
    setSelecting(false);
    setSel(NONE);
  };
  const n = shown.length;
  return (
    <>
      <CardHeader
        title="History"
        help="history"
        meta={n > 0 && counted(shown)}
        action={
          <div className="flex items-center gap-2">
            {entries.length > 0 && (
              <Segmented
                aria-label="Show"
                options={FILTERS}
                value={filter}
                onValueChange={setFilter}
              />
            )}
            {entries.length > 1 && !selecting && (
              <>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setSelecting(true)}
                >
                  Select
                </Button>
                <ClearHistory />
              </>
            )}
          </div>
        }
      />
      {selecting && (
        <SelectBar
          picked={shown.filter((e) => sel.ids.has(e.id))}
          shown={n}
          onSelectAll={() => setSel({ ids: new Set(ids.current) })}
          onDone={done}
        />
      )}
      {n === 0 ? (
        <p className="px-4 pb-4 text-[13px] text-ink-muted">
          {entries.length ? EMPTY[filter] : "No seeds yet."}
        </p>
      ) : (
        <ul className={cn("px-1 pb-1", divided)}>
          {shown.slice(0, drawnNow).map((e) => (
            <HistoryRow
              key={e.id}
              entry={e}
              picked={selecting ? sel.ids.has(e.id) : undefined}
              onToggle={toggle}
            />
          ))}
        </ul>
      )}
      {n > drawn && (
        <div className="flex justify-center px-4 pb-3">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setDrawn(drawn + PAGE)}
          >
            Show {Math.min(PAGE, n - drawn)} more
          </Button>
        </div>
      )}
    </>
  );
}

/** Props for {@link HistoryRow}. */
interface HistoryRowProps {
  entry: Summary;
  /** Whether it's picked, in select mode. `undefined` outside it. */
  picked: boolean | undefined;
  onToggle: (id: number, range: boolean) => void;
}

/** An entry keeps its identity until history changes, so a row redraws only
 * for its own values. */
const HistoryRow = memo(function HistoryRow({
  entry: e,
  picked,
  onToggle,
}: HistoryRowProps) {
  const [installedId] = useSeeds.usePath("history.installed.id");
  const [installingId] = useSeeds.usePath("installing");
  const differs = useDiffCount(e);
  const isInstalled = e.id === installedId;
  const isInstalling = e.id === installingId;
  return (
    <ListRow
      id={rowId.seed(e.id)}
      lead={<SeedIcons icons={e.icons} size="sm" />}
      name={
        nameOf(e) ? (
          <Name entry={e} className="w-fit max-w-full font-medium text-sm" />
        ) : (
          <p className="text-ink-faint text-sm">Unnamed seed</p>
        )
      }
      meta={
        <>
          {e.starred && (
            <Star aria-label="Starred" className={cn("size-3", STARRED)} />
          )}
          {when(e.time)}
          <ExtrasBadges entry={e} />
          {!e.ap_slot && <DiffersIn n={differs} />}
        </>
      }
      select={
        picked !== undefined
          ? {
              label: `Select ${nameOf(e) || `seed ${e.seed}`}`,
              on: picked,
              toggle: (range) => onToggle(e.id, range),
            }
          : undefined
      }
      mark={
        (isInstalled || isInstalling) && (
          <InstallMark
            installing={isInstalling}
            replaced={installingId !== null && !isInstalling}
          />
        )
      }
      actions={
        // Select mode trades the actions for picking.
        picked === undefined && (
          <>
            {!isInstalled && !isInstalling && <InstallSeed id={e.id} />}
            {/* A slot's settings belong to its multiworld, and no code rebuilds it. */}
            {!e.ap_slot && (
              <UseSettings
                differs={differs}
                tip="Use this seed's settings, pins, colors and music for the next one. Flag edits stay behind"
                onUse={() => takeDraft(e)}
              />
            )}
            <TipButton
              label="Spoiler Log"
              tip="Spoiler Log"
              variant="ghost"
              onClick={() => openSpoiler(e)}
            >
              <ScrollText />
            </TipButton>
            {!e.ap_slot && (
              <CopySeed entry={e} variant="ghost" size="icon-sm" />
            )}
            <StarSeed id={e.id} starred={e.starred} />
            <TipButton
              label="Label seed"
              tip="Name this seed or note what it's for"
              variant="ghost"
              onClick={() => openLabel(e)}
            >
              <Pencil />
            </TipButton>
            <DeleteSeed id={e.id} installed={isInstalled} />
          </>
        )
      }
    />
  );
});
