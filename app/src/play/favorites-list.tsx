import { Heart, Pencil, RefreshCw, Trash2 } from "lucide-react";
import { useState } from "react";
import type { Favorite, Label } from "@/bindings";
import { CardHeader } from "@/components/card";
import { RequestError } from "@/components/request-error";
import { divided } from "@/components/styles";
import { TipButton } from "@/components/tip-button";
import { Button } from "@/components/ui/button";
import type { Source } from "@/lib/drift";
import { count, when } from "@/lib/format";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { cn } from "@/lib/utils";
import { NameDialog } from "@/play/label-dialog";
import {
  ConfirmButton,
  DiffersIn,
  ListRow,
  UseSettings,
} from "@/play/list-row";
import { useKept } from "@/play/use-kept";
import { useConfig } from "@/stores/config";
import {
  deleteFavorite,
  relabelFavorite,
  saveFavorite,
  updateFavorite,
} from "@/stores/favorites";
import { takeDraft, useDiffCount } from "@/stores/seeds";

/** What the name dialog is doing: labelling the favorite called `name`, or
 * saving a new one without it. */
interface Naming {
  name?: string;
}

/** What Use settings does with one of the player's own favorites. */
const USE_TIP =
  "Use these settings, pins, colors and music for the next seed. Flag edits stay behind";

/** What Use settings does with a built-in: it keeps no draft of its own, so
 * nothing of yours is touched. */
const BUILT_IN_TIP =
  "Use these settings for the next seed. Your pins, colors and music stay as they are";

/** The Favorites folder: setting sets kept by name, newest first, with the
 * built-in ones pinned below. Its rows' writes share one request, which
 * shows why the last failed. */
export function FavoritesList() {
  return (
    <RequestProvider>
      <Favorites />
    </RequestProvider>
  );
}

function Favorites() {
  const mine = useConfig((s) => s.config.favorites);
  const builtIns = useConfig((s) => s.builtIns);
  const [naming, setNaming] = useState<Naming>();
  // The dialog keeps what it names while it fades out.
  const kept = useKept(naming);
  const labelled = mine.find((f) => kept?.name === f.name);
  return (
    <>
      <CardHeader
        title="Favorites"
        help="favorites"
        meta={mine.length > 0 && count(mine.length, "favorite")}
        action={
          <Button variant="ghost" size="sm" onClick={() => setNaming({})}>
            <Heart />
            Save current
          </Button>
        }
      />
      <RequestError className="mx-4 mb-2" />
      {mine.length === 0 ? (
        <p className="px-4 pb-2 text-[13px] text-ink-muted">
          No favorites yet. Save what you have now, and Use it any time.
        </p>
      ) : (
        <ul aria-label="Your favorites" className={cn("px-1 pb-1", divided)}>
          {mine.map((f) => (
            <FavoriteRow
              key={f.name}
              favorite={f}
              onLabel={() => setNaming({ name: f.name })}
            />
          ))}
        </ul>
      )}
      <p className="px-4 pt-2 font-medium text-ink-faint text-xs">Built in</p>
      <ul aria-label="Built in" className={cn("px-1 pb-1", divided)}>
        {builtIns.map((b) => (
          <ListRow
            key={b.id}
            name={<p className="truncate font-medium text-sm">{b.name}</p>}
            meta={<Meta note={b.note} source={b} />}
            actions={<Use source={b} tip={BUILT_IN_TIP} />}
          />
        ))}
      </ul>
      <NameDialog
        open={naming !== undefined}
        onClose={() => setNaming(undefined)}
        heading={kept?.name ? "Label Favorite" : "Save Favorite"}
        description={
          kept?.name
            ? kept.name
            : "Keeps your settings, pins, colors and music under a name"
        }
        formKey={kept?.name ?? ""}
        label={labelled && { name: labelled.name, note: labelled.note ?? "" }}
        onSave={async (l: Label) => {
          await (kept?.name ? relabelFavorite(kept.name, l) : saveFavorite(l));
        }}
      />
    </>
  );
}

/** Props for {@link Meta}. */
interface MetaProps {
  time?: number;
  note?: string;
  /** The favorite or built-in whose draft this row offers. */
  source: Source;
}

/** A favorite's quiet line: when, its note, and how much differs. */
function Meta({ time, note, source }: MetaProps) {
  const differs = useDiffCount(source);
  const parts = [time !== undefined && when(time), note].filter(Boolean);
  return (
    <>
      <span className="truncate">{parts.join(" · ")}</span>
      <DiffersIn n={differs} />
    </>
  );
}

/** Props for {@link Use}. */
interface UseProps {
  source: Source;
  /** What it says on hover, which differs between the player's own favorites
   * and the built-ins. */
  tip: string;
}

/** Makes `source`'s draft the current one. */
function Use({ source, tip }: UseProps) {
  const differs = useDiffCount(source);
  return (
    <UseSettings differs={differs} tip={tip} onUse={() => takeDraft(source)} />
  );
}

/** Props for {@link FavoriteRow}. */
interface FavoriteRowProps {
  favorite: Favorite;
  onLabel: () => void;
}

/** One of the player's own favorites, with Update, Label and Delete. */
function FavoriteRow({ favorite: f, onLabel }: FavoriteRowProps) {
  const differs = useDiffCount(f);
  const { run } = useRequest();
  return (
    <ListRow
      name={<p className="truncate font-medium text-sm">{f.name}</p>}
      meta={<Meta time={f.time} note={f.note} source={f} />}
      actions={
        <>
          <Use source={f} tip={USE_TIP} />
          <ConfirmButton
            label="Update favorite"
            tip={
              differs === 0
                ? "It already matches your settings, pins, colors and music"
                : "Keep your current settings, pins, colors and music in this favorite instead"
            }
            confirm="Really update?"
            icon={<RefreshCw />}
            disabled={differs === 0}
            onConfirm={() => run(() => updateFavorite(f.name))}
          />
          <TipButton
            label="Label favorite"
            tip="Rename this favorite or note what it's for"
            variant="ghost"
            onClick={onLabel}
          >
            <Pencil />
          </TipButton>
          <ConfirmButton
            label="Delete favorite"
            tip="Delete this favorite"
            confirm="Really delete?"
            icon={<Trash2 />}
            onConfirm={() => run(() => deleteFavorite(f.name))}
          />
        </>
      }
    />
  );
}
