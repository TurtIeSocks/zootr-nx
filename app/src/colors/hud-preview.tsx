import { useEffect, useState } from "react";
import { commands, type PaletteView, type Picked } from "@/bindings";
import { Card } from "@/components/card";
import { Label } from "@/components/ui/label";
import { Switch } from "@/components/ui/switch";
import { call } from "@/lib/api";
import { previewUrl } from "@/lib/colors";
import { useLoadedImage } from "./use-loaded-image";

/** Props for {@link HudPreview}. */
interface HudPreviewProps {
  /** The HUD group's palettes. */
  palettes: PaletteView[];
  /** A palette's colour as the picker has it now, or null for vanilla. */
  picked: (palette: PaletteView) => Picked | null;
}

/** The HUD assembled: hearts, the magic meter, the rupee counter, the
 * pause menu's header and a line of Navi's, each texture painted by the
 * seed's own code and each layout colour laid on as the game tints it.
 * Low health pulses the hearts, Using magic lights the meter's glow. */
export function HudPreview({ palettes, picked }: HudPreviewProps) {
  const [low, setLow] = useState(false);
  const [magic, setMagic] = useState(false);
  const slots = (id: string) => {
    const palette = palettes.find((p) => p.id === id);
    return { palette, picked: palette ? picked(palette) : null };
  };
  const hearts = slots("hearts");
  const meter = slots("magic-meter");
  const text = useSlots(slots("ui-text"))[0];
  const navi = useSlots(slots("navi-prompt"))[0];
  const pulse = useSlots(hearts)[1];
  const heart = useSheet("hearts", 0, hearts.picked);
  const fill = useSheet("magic-meter", 0, meter.picked);
  const glow = useSheet("magic-meter", 1, meter.picked);
  const rupee = useSheet("rupee-icon", 0, slots("rupee-icon").picked);
  const header = useSheet("pause-backdrop", 0, slots("pause-backdrop").picked);

  return (
    <Card title="HUD" description="Every HUD palette at once, as picked">
      <div className="flex flex-col gap-3 p-4">
        <div
          role="img"
          aria-label="The HUD in the picked colours"
          className="relative aspect-video w-full overflow-hidden rounded-md border border-line bg-[radial-gradient(circle_at_50%_30%,#3b4a2a,#141a10)]"
        >
          {header && (
            <div className="absolute inset-x-0 top-0 h-1/4">
              <img src={header} alt="" className="size-full object-cover" />
              <span
                className="absolute inset-x-0 top-1 text-center font-semibold text-xs"
                style={{ color: text }}
              >
                Items
              </span>
            </div>
          )}
          <div className="absolute top-[30%] left-3 flex flex-col gap-1">
            <div className="flex -space-x-1">
              {[0, 1, 2].map((i) => (
                <Heart key={i} src={heart} pulse={low ? pulse : undefined} />
              ))}
            </div>
            <div className="relative h-2 w-28">
              {magic && glow && (
                <img
                  src={glow}
                  alt=""
                  className="absolute inset-0 size-full scale-y-[2.5] blur-[3px]"
                />
              )}
              {fill && (
                <img
                  src={fill}
                  alt=""
                  className="absolute inset-0 size-full rounded-sm"
                />
              )}
            </div>
          </div>
          <div className="absolute top-[30%] right-3 flex items-center gap-1">
            {rupee && <img src={rupee} alt="" className="size-6" />}
            <span className="font-semibold text-sm" style={{ color: text }}>
              0/200
            </span>
          </div>
          <div
            className="absolute inset-x-6 bottom-3 rounded-2xl bg-black/70 px-4 py-2 text-white text-xs"
            style={{ boxShadow: `0 0 10px ${navi}66` }}
          >
            This <span style={{ color: navi }}>Deku Baba</span>... looks kinda
            wilted.
          </div>
        </div>
        <div className="flex flex-wrap gap-4">
          <Toggle id="hud-low" label="Low health" on={low} onChange={setLow} />
          <Toggle
            id="hud-magic"
            label="Using magic"
            on={magic}
            onChange={setMagic}
          />
        </div>
      </div>
    </Card>
  );
}

/** One heart, and over it, at low health, its pulse in `pulse`. */
function Heart({ src, pulse }: { src: string | null; pulse?: string }) {
  if (!src) return <span className="size-8" />;
  const mask = { maskImage: `url("${src}")`, maskSize: "contain" };
  return (
    <span className="relative size-8">
      <img src={src} alt="" className="size-full" />
      {pulse && (
        <span
          aria-hidden
          className="absolute inset-0 animate-pulse mix-blend-screen motion-reduce:animate-none"
          style={{
            ...mask,
            WebkitMaskImage: mask.maskImage,
            background: pulse,
          }}
        />
      )}
    </span>
  );
}

/** Props for {@link Toggle}. */
interface ToggleProps {
  id: string;
  label: string;
  on: boolean;
  onChange: (on: boolean) => void;
}

function Toggle({ id, label, on, onChange }: ToggleProps) {
  return (
    <div className="flex items-center gap-2">
      <Switch id={id} checked={on} onCheckedChange={onChange} />
      <Label htmlFor={id}>{label}</Label>
    </div>
  );
}

/** A palette's sheet, painted with `picked`, once loaded. */
function useSheet(id: string, sheet: number, picked: Picked | null) {
  return useLoadedImage(previewUrl({ id, sheet, picked })).src;
}

/** Each slot's colour: vanilla without a pick, the split colours when
 * split, else the pick and what the others follow it to. */
function useSlots({
  palette,
  picked,
}: {
  palette?: PaletteView;
  picked: Picked | null;
}): string[] {
  const [followed, setFollowed] = useState<string[]>([]);
  const hex = picked && !picked.split?.length ? picked.hex : null;
  const id = palette?.id;
  useEffect(() => {
    if (!id || !hex) return;
    let live = true;
    call(commands.colorsFollow(id, hex)).then((s) => live && setFollowed(s));
    return () => {
      live = false;
    };
  }, [id, hex]);
  if (!palette) return [];
  if (!picked) return palette.vanilla;
  if (picked.split?.length) return [picked.hex, ...picked.split];
  return followed[0] === picked.hex
    ? followed
    : palette.vanilla.map(() => picked.hex);
}
