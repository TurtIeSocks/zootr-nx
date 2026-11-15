import { Fragment, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  type Cell,
  type DungeonRow,
  type IconSize,
  type Placement,
  TRACKER_COLUMNS,
} from "@/bindings";
import { fallbackIcon, isLight } from "@/items/fallback-icons";
import { iconUrl } from "@/items/icons";
import { TITLES } from "@/items/layout";
import { SectionToolbar } from "@/items/section-toolbar";
import { useGained } from "@/items/use-gained";
import { cn } from "@/lib/utils";
import { useItems } from "@/stores/items";

/** A tile's side in pixels, by size. */
const PIXELS: Record<IconSize, number> = { Small: 32, Medium: 48, Large: 72 };

/** Ink for what's drawn straight on the tracker's background: dark on a
 * light one, white on a dark one, whichever contrasts more. */
function useInk() {
  const light = useItems((s) => isLight(s.options.background));
  return light ? "text-neutral-900" : "text-white";
}

/** Gold pips for the tier held, with the number for screen readers. */
function Tier({ cell }: TileProps) {
  return (
    <span className="absolute -bottom-0.75 left-1/2 flex -translate-x-1/2 gap-0.5">
      {Array.from({ length: cell.tiers }, (_, i) => (
        <i
          // biome-ignore lint/suspicious/noArrayIndexKey: pips are only positions
          key={i}
          className={cn(
            "size-1 rounded-full",
            i < cell.tier ? "bg-gold" : "bg-line-strong",
          )}
        />
      ))}
      <span className="sr-only">{cell.tier}</span>
    </span>
  );
}

/** Props for {@link Tile}. */
interface TileProps {
  cell: Cell;
}

/** One item at the chosen size: the dump's icon, a pulse while those
 * build, or zootr-nx's own when game assets are off or the icon won't
 * load. Grey while missing, and fainter still where a dungeon has none. */
function Tile({ cell }: TileProps) {
  const [art] = useItems.usePath("art");
  const size = useItems((s) => PIXELS[s.options.size]);
  const [broken, setBroken] = useState(false);
  const { gained, done } = useGained(cell.tier);
  const absent = cell.tiers === 0;
  const missing = cell.tier === 0;
  const tier = cell.section === "Upgrades" && cell.tiers > 1 && !missing;
  return (
    <div
      role="img"
      aria-label={
        absent
          ? `${cell.label} (not in this dungeon)`
          : missing
            ? `${cell.label} (missing)`
            : cell.label
      }
      title={cell.label}
      className={cn(
        "relative flex items-center justify-center transition-[opacity,filter] duration-(--duration-base)",
        absent ? "opacity-12 grayscale" : missing && "opacity-28 grayscale",
        gained && "animate-gain",
      )}
      style={{ width: size, height: size }}
      onAnimationEnd={done}
    >
      {art === "building" ? (
        <span className="size-full animate-pulse rounded-sm bg-white/15" />
      ) : art === "game" && !broken ? (
        <img
          src={iconUrl(cell.icon)}
          alt=""
          draggable={false}
          className="size-full object-contain"
          onError={() => setBroken(true)}
        />
      ) : (
        <FallbackIcon icon={cell.icon} />
      )}
      {cell.count !== null && !missing && (
        <span className="absolute -right-0.5 -bottom-0.5 inline-flex h-4 min-w-4.5 items-center justify-center rounded-xs bg-ink px-1 font-mono font-semibold text-[10.5px] text-surface leading-none">
          {cell.count}
        </span>
      )}
      {tier && <Tier cell={cell} />}
    </div>
  );
}

/** Props for {@link FallbackIcon}. */
interface FallbackIconProps {
  /** The cell's icon name. */
  icon: string;
}

/** zootr-nx's own icon for a cell, in dark ink on a light background and
 * white on a dark one, so a missing item's faded icon still shows. */
function FallbackIcon({ icon }: FallbackIconProps) {
  const Icon = fallbackIcon(icon);
  return <Icon className={cn("size-[65%] stroke-[1.5]", useInk())} />;
}

/** Props for {@link ItemsGrid}. */
export interface ItemsGridProps {
  /** Puts a layout toolbar on each section. The pop out has none. */
  edit?: boolean;
}

/** The grid from the items store, as both windows draw it. */
export function ItemsGrid({ edit }: ItemsGridProps) {
  const [sections] = useItems.usePath("options.sections");
  const [background] = useItems.usePath("options.background");
  // An empty column isn't drawn, so the pop out trims to what's shown.
  const columns = Array.from({ length: TRACKER_COLUMNS }, (_, i) => ({
    n: i + 1,
    shown: sections.filter((p) => p.shown && p.column === i + 1),
  })).filter((c) => c.shown.length > 0);
  return (
    <div
      className={cn("inline-flex items-start gap-3 p-3", edit && "rounded-lg")}
      style={{ background }}
    >
      {columns.map(({ n, shown }) => (
        <div key={n} className="flex flex-col gap-3">
          {shown.map((placement) => (
            <GridSection
              key={placement.section}
              placement={placement}
              edit={edit}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

/** Props for {@link GridSection}. */
interface GridSectionProps extends ItemsGridProps {
  placement: Placement;
}

/** Props for {@link RowLabel}. */
interface RowLabelProps {
  row: DungeonRow;
  /** The section title's ink. */
  ink: string;
}

/** The short name that starts a dungeon's row, the full one on hover. The
 * tiles already name it, so screen readers skip it. */
function RowLabel({ row, ink }: RowLabelProps) {
  return (
    <span
      aria-hidden
      title={row.name}
      className={cn(
        "self-center pr-1 font-semibold text-[10.5px] leading-3.5 tracking-[0.04em] opacity-80",
        ink,
      )}
    >
      {row.short}
    </span>
  );
}

/** A titled section of tiles, its width in tiles. A section laid out in
 * rows starts each with its label, and keeps its width. */
function GridSection({ placement, edit }: GridSectionProps) {
  const { section, width } = placement;
  const cells = useItems(
    useShallow((s) => s.cells.filter((c) => c.section === section)),
  );
  const size = useItems((s) => PIXELS[s.options.size]);
  const ink = useInk();
  const rows = cells.some((c) => c.row);
  return (
    <section
      aria-label={TITLES[section]}
      className={cn(
        "group relative",
        // An outline takes no room, so the tab matches the pop out.
        edit &&
          "rounded-sm outline-1 outline-transparent outline-dashed outline-offset-4 transition-[outline-color] duration-(--duration-fast) focus-within:outline-line-strong hover:outline-line-strong",
      )}
    >
      {edit && (
        <SectionToolbar
          placement={placement}
          slots={cells.length}
          fixed={rows}
        />
      )}
      <h3
        className={cn(
          "mb-1.5 ml-0.5 font-semibold text-[10.5px] uppercase leading-3.5 tracking-[0.08em] opacity-80",
          ink,
        )}
      >
        {TITLES[section]}
      </h3>
      <div
        className="grid gap-1"
        style={{
          gridTemplateColumns: `${rows ? "auto " : ""}repeat(${width}, ${size}px)`,
        }}
      >
        {cells.map((cell, i) => (
          // Bottles share a label, so the position is the key.
          // biome-ignore lint/suspicious/noArrayIndexKey: fixed grid order
          <Fragment key={i}>
            {cell.row && <RowLabel row={cell.row} ink={ink} />}
            <Tile cell={cell} />
          </Fragment>
        ))}
      </div>
    </section>
  );
}
