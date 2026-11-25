import { useEffect, useState } from "react";
import type { PaletteView, Picked } from "@/bindings";
import { Card } from "@/components/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { previewUrl } from "@/lib/colors";
import { cn } from "@/lib/utils";
import { useLoadedImage } from "./use-loaded-image";

/** A corner label on the wipe's sides. */
const TAG =
  "absolute top-2 rounded bg-black/60 px-1.5 py-0.5 text-white text-xs";

/** Props for {@link PreviewPanel}. */
interface PreviewPanelProps {
  palette: PaletteView;
  /** The colour to paint, as the picker has it now, or null for vanilla. */
  picked: Picked | null;
  /** The colour as saved, which the texture thumbnails show: they skip
   * the drag's steps, so several textures don't flood the backend. */
  saved: Picked | null;
  /** A line under the title: why it shows vanilla, say. */
  note?: string;
}

/** The selected palette's game texture, painted by the seed's own code as
 * the picker moves. A wipe slides between vanilla and the pick, What
 * Changes greys the texels the palette leaves alone, and a palette that
 * paints several textures shows each. */
export function PreviewPanel({
  palette,
  picked,
  saved,
  note,
}: PreviewPanelProps) {
  const [texture, setTexture] = useState(0);
  const [changes, setChanges] = useState(false);
  /** The divider, as a percent from the left: vanilla shows left of it,
   * the pick right of it. At 0 the pick fills the frame. */
  const [wipe, setWipe] = useState(0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: a new palette starts at its first texture
  useEffect(() => setTexture(0), [palette.id]);
  const t = Math.min(texture, palette.textures - 1);
  const shot = { id: palette.id, texture: t, changes };
  const after = useLoadedImage(previewUrl({ ...shot, picked }));
  const before = useLoadedImage(previewUrl({ ...shot, picked: null }));

  return (
    <Card title={palette.label} description={note}>
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
              {wipe < 100 && <span className={cn(TAG, "right-2")}>Picked</span>}
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
        {palette.textures > 1 && (
          <fieldset className="grid grid-cols-6 gap-1">
            <legend className="sr-only">Texture</legend>
            {Array.from({ length: palette.textures }, (_, i) => (
              <Thumb
                // biome-ignore lint/suspicious/noArrayIndexKey: textures are numbered
                key={i}
                label={`Texture ${i + 1} of ${palette.textures}`}
                url={previewUrl({ id: palette.id, texture: i, picked: saved })}
                pressed={i === t}
                onPress={() => setTexture(i)}
              />
            ))}
          </fieldset>
        )}
      </div>
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

/** One of a palette's textures, small: pressing it shows it large. */
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
