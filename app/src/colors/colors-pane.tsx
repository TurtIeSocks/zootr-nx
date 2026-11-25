import {
  Heart,
  Lock,
  LockOpen,
  RotateCcw,
  Shirt,
  Sparkles,
  SplitSquareHorizontal,
  Sword,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import {
  commands,
  type Group,
  type PaletteView,
  type Pick,
  type Picked,
} from "@/bindings";
import { FolderTabs } from "@/components/folder-tabs";
import { RequestError } from "@/components/request-error";
import { SavedSets } from "@/components/saved-sets";
import { Tip } from "@/components/tip";
import { Button } from "@/components/ui/button";
import { ColorPicker } from "@/components/ui/color-picker";
import { Segmented } from "@/components/ui/segmented";
import { call, message } from "@/lib/api";
import {
  type Mode,
  modeOf,
  PRESETS,
  setAll,
  setPick,
  swatchOf,
  toggleLock,
  vanillaLabel,
  withSlot,
} from "@/lib/colors";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { cn } from "@/lib/utils";
import { useSaveConfig } from "@/settings/use-save-config";
import { PageHeader } from "@/shell/page-header";
import { saveConfig, useConfig } from "@/stores/config";
import { applyCosmetics, useSeeds } from "@/stores/seeds";
import { HudPreview } from "./hud-preview";
import { PreviewPanel } from "./preview-panel";

const GROUPS: { id: Group; label: string; icon: typeof Shirt }[] = [
  { id: "link", label: "Link", icon: Shirt },
  { id: "gear", label: "Gear", icon: Sword },
  { id: "effects", label: "Effects", icon: Sparkles },
  { id: "hud", label: "HUD", icon: Heart },
];

/** A palette's three-way switch, its first option named for it. */
const modes = (palette: PaletteView): { id: Mode; label: string }[] => [
  { id: "vanilla", label: vanillaLabel(palette) },
  { id: "random", label: "Random" },
  { id: "colour", label: "Colour" },
];

const SWATCHES = PRESETS.map(([, hex]) => hex);

/** How long a dragged colour waits before it saves: the preview follows
 * at once, the config file only once the hand rests. */
const SAVE_AFTER_MS = 250;

/** The palettes a seed can recolour, each set to Vanilla, Random or a
 * colour, with a live preview of the selected one. */
export function ColorsPane() {
  const catalog = useCatalog();
  return (
    <RequestProvider>
      <PageHeader
        title="Colors"
        help="colors"
        description="Recolour Link, his gear, Navi and the HUD. The next seed you generate is painted this way; Apply Cosmetics paints the installed seed again without a new shuffle. Each change saves as you make it."
        actions={<Actions palettes={catalog.palettes} />}
      />
      <SavedSets
        kind="colors"
        id="colors-saved"
        words={{
          label: "Saved Color Sets",
          hint: "Picking one loads its colours below, replacing them. Save keeps the colours below under the name, replacing a set with that name",
          noun: "color set",
          unit: "colour",
          empty: "Every palette is vanilla",
          unsaved: "Not saved. The next seed still gets these colours",
        }}
      />
      <Palettes {...catalog} />
      <RequestError />
    </RequestProvider>
  );
}

/** Every palette, and why the catalog couldn't be read. */
function useCatalog() {
  const [game] = useConfig.usePath("config.game");
  const [art] = useConfig.usePath("config.game_assets");
  const [palettes, setPalettes] = useState<PaletteView[]>([]);
  const [error, setError] = useState<string>();
  // biome-ignore lint/correctness/useExhaustiveDependencies: checked again against a new dump or with game assets turned on
  useEffect(() => {
    let live = true;
    call(commands.colorsCatalog())
      .then((p) => live && setPalettes(p))
      .catch((e) => live && setError(message(e)));
    return () => {
      live = false;
    };
  }, [game, art]);
  return { palettes, error };
}

/** The ids Randomize all and Reset all may change: every palette this
 * dump can paint. */
const paintable = (palettes: PaletteView[]) =>
  palettes.filter((p) => !p.problem).map((p) => p.id);

/** Randomize all, Reset all, and Apply to installed seed. */
function Actions({ palettes }: { palettes: PaletteView[] }) {
  const store = useSaveConfig();
  const { busy, run } = useRequest();
  const installed = useSeeds((s) => s.history.installed);
  const blocked = useSeeds((s) => s.blocked);
  const generating = useSeeds((s) => s.generating);
  const ids = paintable(palettes);
  // A palette that follows another keeps following it: brows with the hair.
  const leaders = paintable(palettes.filter((p) => !p.follows));
  // One request: a save nested in `run` would be dropped as already running.
  const randomize = (drawNow: boolean) =>
    run(async () => {
      const drawn = drawNow ? await call(commands.colorsDraw(leaders)) : {};
      await saveConfig(
        setAll(leaders, (id) =>
          drawNow ? { hex: drawn[id] ?? "#808080" } : "random",
        ),
      );
    });
  const why = !installed ? "No seed is installed" : blocked;
  return (
    <div className="flex flex-wrap gap-2">
      <Tip tip="Shift-click to draw the colours now, to see and tweak them">
        <Button
          variant="secondary"
          disabled={busy}
          onClick={(e) => randomize(e.shiftKey)}
        >
          Randomize All
        </Button>
      </Tip>
      <Button
        variant="secondary"
        disabled={busy}
        onClick={() => store(setAll(ids, () => "vanilla"))}
      >
        Reset All
      </Button>
      <Tip
        tip={why ?? "Plays the Colors and Audio panes in the installed seed"}
        wrap
      >
        <Button
          disabled={busy || generating || !!why}
          onClick={() => run(applyCosmetics)}
        >
          Apply Cosmetics to Seed
        </Button>
      </Tip>
    </div>
  );
}

/** The palettes by group, and the preview beside them. */
function Palettes({ palettes, error }: ReturnType<typeof useCatalog>) {
  const [colors] = useConfig.usePath("config.colors");
  const [art] = useConfig.usePath("config.game_assets");
  const [game] = useConfig.usePath("config.game");
  const [group, setGroup] = useState<Group>("link");
  const [selected, setSelected] = useState<string>();
  /** Colours the pickers hold that haven't saved yet, by palette. */
  const [live, setLive] = useState<Record<string, Picked>>({});
  const store = useSaveConfig();
  const timers = useRef<Record<string, number>>({});

  // A saved draft (a loaded set, Reset all) wins over a held colour.
  // biome-ignore lint/correctness/useExhaustiveDependencies: clears whenever the saved draft changes
  useEffect(() => setLive({}), [colors]);

  const pickLive = (id: string, picked: Picked) => {
    setLive((l) => ({ ...l, [id]: picked }));
    window.clearTimeout(timers.current[id]);
    timers.current[id] = window.setTimeout(
      () => void store(setPick(id, picked)),
      SAVE_AFTER_MS,
    );
  };

  const shown = palettes.filter((p) => p.group === group);
  const current =
    shown.find((p) => p.id === selected) ?? (shown[0] as PaletteView);
  const saved = (p: PaletteView): Picked | null => {
    const pick = colors[p.id];
    return typeof pick === "object" ? pick : null;
  };
  const picked = (p: PaletteView): Picked | null => live[p.id] ?? saved(p);
  const previews = art && game !== null;

  const panel = (
    <div className="grid gap-4 p-4 lg:grid-cols-[minmax(0,1fr)_320px]">
      <ul className="flex flex-col divide-y divide-line">
        {shown.map((p) => (
          <PaletteRow
            key={p.id}
            palette={p}
            pick={colors[p.id]}
            live={live[p.id]}
            selected={p.id === current?.id}
            onSelect={() => setSelected(p.id)}
            onLive={(picked) => pickLive(p.id, picked)}
          />
        ))}
      </ul>
      {current && (
        <div className="lg:sticky lg:top-4 lg:self-start">
          {previews && group === "hud" ? (
            <HudPreview palettes={shown} picked={picked} />
          ) : previews ? (
            <PreviewPanel
              palette={current}
              picked={picked(current)}
              saved={saved(current)}
              note={
                colors[current.id] === "random"
                  ? "Random: drawn when the seed is built. Shown in vanilla"
                  : undefined
              }
            />
          ) : (
            <p className="rounded-md border border-line border-dashed p-4 text-ink-muted text-sm">
              Turn on game assets in Settings to preview each palette on its
              game texture.
            </p>
          )}
        </div>
      )}
    </div>
  );

  if (error) return <p className="text-danger text-sm">{error}</p>;
  return (
    <FolderTabs
      label="Palette groups"
      value={group}
      onValueChange={setGroup}
      tabs={GROUPS.map((g) => ({ ...g, panel }))}
    />
  );
}

/** Props for {@link PaletteRow}. */
interface PaletteRowProps {
  palette: PaletteView;
  pick: Pick | undefined;
  /** The colours the pickers hold before they save. */
  live?: Picked;
  selected: boolean;
  onSelect: () => void;
  onLive: (picked: Picked) => void;
}

/** One palette: its name, Vanilla, Random or a colour, the picker, its
 * lock and reset. A palette with several slots can Split them, a picker
 * each. Hovering or focusing it shows it in the preview. */
function PaletteRow({
  palette,
  pick,
  live,
  selected,
  onSelect,
  onLive,
}: PaletteRowProps) {
  const store = useSaveConfig();
  const { busy, run } = useRequest();
  const locked = useConfig((s) => s.config.color_locks.includes(palette.id));
  const mode = live ? "colour" : modeOf(pick);
  const picked: Picked =
    live ??
    (typeof pick === "object" ? pick : { hex: swatchOf(palette, pick) });
  const split = !!picked.split?.length;
  const off = busy || !!palette.problem;
  const setMode = (m: Mode) =>
    store(setPick(palette.id, m === "colour" ? { hex: picked.hex } : m));
  // Split starts from where the slots stood, following the first.
  const toggleSplit = () =>
    run(async () => {
      const slots = split
        ? []
        : (await call(commands.colorsFollow(palette.id, picked.hex))).slice(1);
      await saveConfig(setPick(palette.id, { hex: picked.hex, split: slots }));
    });
  const slots = split ? [picked.hex, ...(picked.split ?? [])] : [picked.hex];
  return (
    <li
      onMouseEnter={onSelect}
      onFocusCapture={onSelect}
      className={cn(
        "flex flex-col gap-2 rounded-md px-3 py-2.5 transition-colors",
        selected && "bg-surface-hover",
        palette.problem && "opacity-60",
      )}
    >
      <div className="flex items-center gap-1">
        <span className="flex-1 font-medium text-sm">{palette.label}</span>
        <Tip tip={locked ? "Unlock" : "Lock against Randomize and Reset All"}>
          <Button
            size="icon"
            variant="ghost"
            aria-pressed={locked}
            aria-label={`Lock ${palette.label}`}
            disabled={busy}
            onClick={() => store(toggleLock(palette.id))}
          >
            {locked ? <Lock /> : <LockOpen className="text-ink-faint" />}
          </Button>
        </Tip>
        <Tip tip="Back to vanilla">
          <Button
            size="icon"
            variant="ghost"
            aria-label={`Reset ${palette.label}`}
            disabled={off || mode === "vanilla"}
            onClick={() => store(setPick(palette.id, "vanilla"))}
          >
            <RotateCcw />
          </Button>
        </Tip>
      </div>
      <div className="flex flex-wrap items-center gap-3">
        <Segmented
          options={modes(palette)}
          value={mode}
          onValueChange={setMode}
          disabled={off}
          aria-label={`${palette.label}: ${vanillaLabel(palette).toLowerCase()}, random or a colour`}
        />
        {mode === "random" ? (
          <span className="text-ink-muted text-xs">Drawn at build</span>
        ) : (
          slots.map((hex, i) => (
            <Swatch
              // biome-ignore lint/suspicious/noArrayIndexKey: slots are numbered
              key={i}
              hex={hex}
              name={split ? palette.names[i] : undefined}
              label={`${palette.label} ${split ? palette.names[i] : "colour"}`}
              onChange={(h) => onLive(withSlot(picked, i, h))}
            />
          ))
        )}
        {palette.names.length > 1 && mode === "colour" && (
          <Tip
            tip={
              split
                ? "One colour, the rest following it"
                : `A colour for each: ${palette.names.join(", ")}`
            }
          >
            <Button
              size="sm"
              variant="ghost"
              aria-pressed={split}
              disabled={off}
              onClick={toggleSplit}
            >
              <SplitSquareHorizontal />
              Split
            </Button>
          </Tip>
        )}
      </div>
      {palette.problem && (
        <p className="w-full text-danger text-xs">
          This dump can't be painted here: {palette.problem}
        </p>
      )}
    </li>
  );
}

/** Props for {@link Swatch}. */
interface SwatchProps {
  hex: string;
  /** The slot's name, shown over it when the palette is split. */
  name?: string;
  label: string;
  onChange: (hex: string) => void;
}

/** One colour: the picker with its presets and recent colours, and its
 * code. */
function Swatch({ hex, name, label, onChange }: SwatchProps) {
  const store = useSaveConfig();
  const [recents] = useConfig.usePath("config.color_recents");
  return (
    <div className="flex flex-col gap-0.5">
      {name && <span className="text-ink-muted text-xs">{name}</span>}
      <div className="flex w-32 items-center gap-2">
        <ColorPicker
          mode="hex"
          alpha={false}
          value={hex}
          onChange={onChange}
          presets={SWATCHES}
          history={recents as `#${string}`[]}
          onHistoryChange={(h) => store({ color_recents: h })}
          aria-label={label}
        />
        <code className="text-ink-muted text-xs">{hex}</code>
      </div>
    </div>
  );
}
