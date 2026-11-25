import {
  CircleCheck,
  Download,
  Loader2,
  Pencil,
  ScrollText,
  Trash2,
} from "lucide-react";
import { memo } from "react";
import type { Summary } from "@/bindings";
import { CardHeader } from "@/components/card";
import { CopySeed, SeedIcons } from "@/components/seed-icons";
import { divided } from "@/components/styles";
import { Tip } from "@/components/tip";
import { TipButton } from "@/components/tip-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Segmented } from "@/components/ui/segmented";
import { nameOf, when } from "@/lib/format";
import { cn } from "@/lib/utils";
import {
  ConfirmButton,
  DiffersIn,
  ListRow,
  UseSettings,
} from "@/play/list-row";
import { ExtrasBadges, Name } from "@/play/seed";
import { useArmed } from "@/play/use-armed";
import { rowId } from "@/shell/find/reveal";
import {
  clearHistory,
  deleteSeed,
  install,
  takeSettings,
  useDiffCount,
  useSeeds,
} from "@/stores/seeds";
import { openLabel, openSpoiler } from "@/stores/shell";
import { type HistoryFilter, useUi } from "@/stores/ui";

/** Deletes every seed but the installed one, on a second click. */
function ClearHistory() {
  const { armed, ...handlers } = useArmed(clearHistory);
  return (
    <Tip tip="Delete every seed except the installed one">
      <Button variant={armed ? "destructive" : "ghost"} size="sm" {...handlers}>
        {armed ? "Really clear?" : "Clear"}
      </Button>
    </Tip>
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
      onConfirm={() => deleteSeed(id)}
    />
  );
}

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
] as const satisfies readonly { id: HistoryFilter; label: string }[];

/** What a filter that keeps no seed says. */
const EMPTY: Record<HistoryFilter, string> = {
  all: "No seeds yet.",
  standard: "No standard seeds yet.",
  archipelago: "No Archipelago slots yet.",
};

/** The History folder: every generated seed, newest first, with its
 * actions a hover away. */
export function HistoryList() {
  const [entries] = useSeeds.usePath("history.entries");
  const [filter, setFilter] = useUi.usePath("play.history");
  const shown = entries.filter(
    (e) => filter === "all" || (filter === "archipelago") === !!e.ap_slot,
  );
  const n = shown.length;
  return (
    <>
      <CardHeader
        title="History"
        help="history"
        meta={n > 0 && `${n} seed${n === 1 ? "" : "s"}`}
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
            {entries.length > 1 && <ClearHistory />}
          </div>
        }
      />
      {n === 0 ? (
        <p className="px-4 pb-4 text-[13px] text-ink-muted">
          {entries.length ? EMPTY[filter] : "No seeds yet."}
        </p>
      ) : (
        <ul className={cn("px-1 pb-1", divided)}>
          {shown.map((e) => (
            <HistoryRow key={e.id} entry={e} />
          ))}
        </ul>
      )}
    </>
  );
}

/** Props for {@link HistoryRow}. */
interface HistoryRowProps {
  entry: Summary;
}

/** An entry keeps its identity until history changes, so a row redraws only
 * for its own values. */
const HistoryRow = memo(function HistoryRow({ entry: e }: HistoryRowProps) {
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
          {when(e.time)}
          <ExtrasBadges entry={e} />
          {!e.ap_slot && <DiffersIn n={differs} />}
        </>
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
        <>
          {!isInstalled && !isInstalling && <InstallSeed id={e.id} />}
          {/* A slot's settings belong to its multiworld, and no code rebuilds it. */}
          {!e.ap_slot && (
            <UseSettings
              differs={differs}
              tip="Use this seed's settings for the next one. Plando and flag edits stay behind"
              onUse={() => takeSettings(e)}
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
          {!e.ap_slot && <CopySeed entry={e} variant="ghost" size="icon-sm" />}
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
      }
    />
  );
});
