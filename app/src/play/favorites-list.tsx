import { Heart, Pencil, RefreshCw, Trash2 } from "lucide-react";
import { useState } from "react";
import type { Favorite, Label } from "@/bindings";
import { CardHeader } from "@/components/card";
import { RequestError } from "@/components/request-error";
import { divided } from "@/components/styles";
import { TipButton } from "@/components/tip-button";
import { Button } from "@/components/ui/button";
import type { Settings } from "@/lib/config";
import { count, when } from "@/lib/format";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { settingsChanges } from "@/lib/settings";
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
import { takeSettings } from "@/stores/seeds";

/** What the name dialog is doing: labelling the favorite called `name`, or
 * saving a new one without it. */
interface Naming {
  name?: string;
}

/** How many settings differ between `settings` and the current ones. */
const useDiffers = (settings: Settings) =>
  useConfig(
    (s) => settingsChanges(s.schema, settings, s.config.settings).length,
  );

/** What Use settings says it does. */
const USE_TIP =
  "Use these settings for the next seed. Plando and flag edits stay behind";

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
          No favorites yet. Save your current settings to keep them here.
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
            meta={<Meta note={b.note} settings={b.settings} />}
            actions={<Use settings={b.settings} />}
          />
        ))}
      </ul>
      <NameDialog
        open={naming !== undefined}
        onClose={() => setNaming(undefined)}
        heading={kept?.name ? "Label Favorite" : "Save Favorite"}
        description={
          kept?.name ? kept.name : "Keeps your current settings under a name"
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
  settings: Settings;
}

/** A favorite's quiet line: when, its note, and how many settings differ. */
function Meta({ time, note, settings }: MetaProps) {
  const differs = useDiffers(settings);
  const parts = [time !== undefined && when(time), note].filter(Boolean);
  return (
    <>
      <span className="truncate">{parts.join(" · ")}</span>
      <DiffersIn n={differs} />
    </>
  );
}

/** Makes `settings` the current ones. */
function Use({ settings }: { settings: Settings }) {
  return (
    <UseSettings
      differs={useDiffers(settings)}
      tip={USE_TIP}
      onUse={() => takeSettings({ settings })}
    />
  );
}

/** Props for {@link FavoriteRow}. */
interface FavoriteRowProps {
  favorite: Favorite;
  onLabel: () => void;
}

/** One of the player's own favorites, with Update, Label and Delete. */
function FavoriteRow({ favorite: f, onLabel }: FavoriteRowProps) {
  const differs = useDiffers(f.settings);
  const { run } = useRequest();
  return (
    <ListRow
      name={<p className="truncate font-medium text-sm">{f.name}</p>}
      meta={<Meta time={f.time} note={f.note} settings={f.settings} />}
      actions={
        <>
          <Use settings={f.settings} />
          <ConfirmButton
            label="Update favorite"
            tip={
              differs === 0
                ? "Your settings already match"
                : "Keep your current settings in this favorite instead"
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
