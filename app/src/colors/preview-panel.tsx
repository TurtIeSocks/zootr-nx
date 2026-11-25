import { useEffect, useState } from "react";
import type { PaletteView, Picked } from "@/bindings";
import { Card } from "@/components/card";
import { Label } from "@/components/ui/label";
import { Segmented } from "@/components/ui/segmented";
import { Switch } from "@/components/ui/switch";
import { previewUrl } from "@/lib/colors";
import { cn } from "@/lib/utils";
import { ModelView } from "./model-view";
import { useLoadedImage } from "./use-loaded-image";

/** What the panel shows: the palette's textures, or its model. */
type Tab = "texture" | "model";

const TABS: readonly { id: Tab; label: string }[] = [
  { id: "texture", label: "Texture" },
  { id: "model", label: "Model" },
];

/** A corner label on the wipe's sides. */
const TAG =
  "absolute top-2 rounded bg-black/60 px-1.5 py-0.5 text-white text-xs";

/** Props for {@link PreviewPanel}. */
interface PreviewPanelProps {
  palette: PaletteView;
  /** The colour to paint, as the picker has it now, or null for vanilla. */
  picked: Picked | null;
  /** The colour as saved, which the thumbnails show: they skip the
   * drag's steps, so several sheets don't flood the backend. */
  saved: Picked | null;
  /** A line under the title: why it shows vanilla, say. */
  note?: string;
}

/** The selected palette's game texture, painted by the seed's own code as
 * the picker moves, or a sphere lit with a material's colours where it
 * writes params. A wipe slides between vanilla and the pick, What Changes
 * greys the texels the palette leaves alone, and a palette with several
 * sheets shows each. */
export function PreviewPanel({
  palette,
  picked,
  saved,
  note,
}: PreviewPanelProps) {
  const [sheet, setSheet] = useState(0);
  const [changes, setChanges] = useState(false);
  /** The divider, as a percent from the left: vanilla shows left of it,
   * the pick right of it. At 0 the pick fills the frame. */
  const [wipe, setWipe] = useState(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new palette starts at its first sheet
  useEffect(() => setSheet(0), [palette.id]);
  const t = Math.min(sheet, palette.sheets - 1);
  const shot = { id: palette.id, sheet: t, changes };
  const after = useLoadedImage(previewUrl({ ...shot, picked }));
  const before = useLoadedImage(previewUrl({ ...shot, picked: null }));
  const [tab, setTab] = useState<Tab>("texture");
  const onModel = tab === "model" && palette.views.length > 0;

  return (
    <Card title={palette.label} description={note}>
      {palette.views.length > 0 && (
        <div className="px-4 pt-4">
          <Segmented
            full
            options={TABS}
            value={onModel ? "model" : "texture"}
            onValueChange={setTab}
          />
        </div>
      )}
      {onModel ? (
        <div className="p-4">
          <ModelView palette={palette} picked={picked} />
        </div>
      ) : (
        <div className="flex flex-col gap-3 p-4">
          <div className="relative aspect-square w-full overflow-hidden rounded-md border border-line bg-surface-sunken">
            {before.src && (
              <img
                src={before.src}
                alt={`${palette.label} in vanilla colours`}
                className="absolute inset-0 size-full object-contain"
              />
            )}
            {after.src && (
              <img
                src={after.src}
                alt={`${palette.label} in the picked colour`}
                style={{ clipPath: `inset(0 0 0 ${wipe}%)` }}
                className="absolute inset-0 size-full object-contain"
              />
            )}
            {wipe > 0 && (
              <>
                <div
                  aria-hidden
                  className="absolute inset-y-0 w-0.5 bg-white/80 shadow"
                  style={{ left: `${wipe}%` }}
                />
                <span className={cn(TAG, "left-2")}>Vanilla</span>
                {wipe < 100 && (
                  <span className={cn(TAG, "right-2")}>Picked</span>
                )}
              </>
            )}
            {(after.failed || before.failed) && !after.src && (
              <p className="absolute inset-0 grid place-items-center p-4 text-center text-ink-muted text-sm">
                This preview couldn't be drawn
              </p>
            )}
          </div>
          <input
            type="range"
            min={0}
            max={100}
            value={wipe}
            aria-label="Uncover vanilla from the left"
            onChange={(e) => setWipe(Number(e.target.value))}
            className="w-full accent-primary"
          />
          <div className="flex items-center gap-2">
            <Switch
              id="colors-changes"
              checked={changes}
              onCheckedChange={setChanges}
            />
            <Label htmlFor="colors-changes">What changes</Label>
          </div>
          {palette.sheets > 1 && (
            <fieldset className="grid grid-cols-6 gap-1">
              <legend className="sr-only">Texture</legend>
              {Array.from({ length: palette.sheets }, (_, i) => (
                <Thumb
                  // biome-ignore lint/suspicious/noArrayIndexKey: sheets are numbered
                  key={i}
                  label={`Preview ${i + 1} of ${palette.sheets}`}
                  url={previewUrl({ id: palette.id, sheet: i, picked: saved })}
                  pressed={i === t}
                  onPress={() => setSheet(i)}
                />
              ))}
            </fieldset>
          )}
        </div>
      )}
    </Card>
  );
}

/** Props for {@link Thumb}. */
interface ThumbProps {
  label: string;
  url: string;
  pressed: boolean;
  onPress: () => void;
}

/** One of a palette's sheets, small: pressing it shows it large. */
function Thumb({ label, url, pressed, onPress }: ThumbProps) {
  const { src } = useLoadedImage(url);
  return (
    <button
      type="button"
      aria-pressed={pressed}
      aria-label={label}
      onClick={onPress}
      className={cn(
        "aspect-square cursor-pointer overflow-hidden rounded border border-line bg-surface-sunken transition-colors hover:border-ink-muted",
        pressed && "border-primary ring-1 ring-primary",
      )}
    >
      {src && <img src={src} alt="" className="size-full object-contain" />}
    </button>
  );
}
