import { Fragment, type ReactNode, useState } from "react";
import type { Cell, DungeonRow, Glyph, Placement, Section } from "@/bindings";
import { Tip } from "@/components/tip";
import { fallbackIcon } from "@/items/fallback-icons";
import { iconUrl } from "@/items/icons";
import { BADGE, TITLES } from "@/items/layout";
import { cn } from "@/lib/utils";
import { useItems } from "@/stores/items";

/** The sections whose progressive items show their tier as pips: the
 * upgrades, and the Magic Meter and Doubled Magic in Quest. */
const PIPPED: Section[] = ["Upgrades", "Quest"];

/** Gold pips for the tier held, with the number for screen readers. */
function Tier({ cell }: { cell: Cell }) {
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
export interface TileProps {
  cell: Cell;
  /** Its side in pixels. */
  size: number;
  /** Ink for zootr-nx's own icons, against what's behind the tile. */
  ink: string;
  /** Leaves the count off, for a grid that draws its own. */
  noCount?: boolean;
  className?: string;
  onAnimationEnd?: () => void;
}

/** One item's {@link ItemIcon}, grey while missing, and fainter still
 * where a dungeon has none. */
export function Tile({
  cell,
  size,
  ink,
  noCount,
  className,
  onAnimationEnd,
}: TileProps) {
  const absent = cell.tiers === 0;
  const missing = cell.tier === 0;
  const pipped = PIPPED.includes(cell.section) && cell.tiers > 1 && !missing;
  return (
    <Tip tip={cell.label}>
      <div
        role="img"
        aria-label={
          absent
            ? `${cell.label} (not in this dungeon)`
            : missing
              ? `${cell.label} (missing)`
              : cell.label
        }
        className={cn(
          "relative flex items-center justify-center transition-[opacity,filter] duration-(--duration-base)",
          absent ? "opacity-12 grayscale" : missing && "opacity-28 grayscale",
          className,
        )}
        style={{ width: size, height: size }}
        onAnimationEnd={onAnimationEnd}
      >
        <ItemIcon icon={cell.icon} glyph={cell.glyph} ink={ink} />
        {!noCount && cell.count !== null && !missing && (
          <span className={BADGE}>{cell.count}</span>
        )}
        {pipped && <Tier cell={cell} />}
      </div>
    </Tip>
  );
}

/** Props for {@link ItemIcon}. */
interface ItemIconProps {
  /** The dump's name for it. */
  icon: string;
  glyph: Glyph;
  ink: string;
}

/** The dump's icon, a pulse while those build, or zootr-nx's own when
 * game assets are off or the icon won't load. Fills its parent. */
export function ItemIcon({ icon, glyph, ink }: ItemIconProps) {
  const [art] = useItems.usePath("art");
  if (art === "building") {
    return (
      <span
        className={cn("size-full animate-pulse rounded-sm bg-current/15", ink)}
      />
    );
  }
  if (art === "game") {
    return <GameIcon key={icon} icon={icon} glyph={glyph} ink={ink} />;
  }
  return <FallbackIcon glyph={glyph} ink={ink} />;
}

/** The dump's icon, or zootr-nx's own once it won't load. Keyed by the
 * icon, and gone during a build, so a new icon or build tries again. */
function GameIcon({ icon, glyph, ink }: ItemIconProps) {
  const [broken, setBroken] = useState(false);
  if (broken) return <FallbackIcon glyph={glyph} ink={ink} />;
  return (
    <img
      src={iconUrl(icon)}
      alt=""
      draggable={false}
      className="size-full object-contain"
      onError={() => setBroken(true)}
    />
  );
}

/** zootr-nx's own icon for a cell, in the grid's ink, so a missing item's
 * faded icon still shows. */
function FallbackIcon({ glyph, ink }: { glyph: Glyph; ink: string }) {
  const Icon = fallbackIcon(glyph);
  return <Icon className={cn("size-[65%] stroke-[1.5]", ink)} />;
}

/** Props for {@link SectionGrid}. */
export interface SectionGridProps {
  placement: Placement;
  /** The section's cells, each with where it sits among all the cells. */
  cells: { cell: Cell; at: number }[];
  size: number;
  ink: string;
  /** Draws one cell. */
  children: (cell: Cell, at: number) => ReactNode;
}

/** A section's title and its cells, `placement.width` to a row. A section
 * laid out in rows starts each with its label, and keeps its width. */
export function SectionGrid({
  placement,
  cells,
  size,
  ink,
  children,
}: SectionGridProps) {
  const rows = cells.some(({ cell }) => cell.row);
  return (
    <>
      <h3
        className={cn(
          "mb-1.5 ml-0.5 font-semibold text-[10.5px] uppercase leading-3.5 tracking-[0.08em] opacity-80",
          ink,
        )}
      >
        {TITLES[placement.section]}
      </h3>
      {/* As wide as its tiles, so a row's label doesn't take the slack. */}
      <div
        className="grid w-max gap-1"
        style={{
          gridTemplateColumns: `${rows ? "auto " : ""}repeat(${placement.width}, ${size}px)`,
        }}
      >
        {cells.map(({ cell, at }) => (
          // Bottles share a label, so the position is the key.
          <Fragment key={at}>
            {cell.row && <RowLabel row={cell.row} ink={ink} />}
            {children(cell, at)}
          </Fragment>
        ))}
      </div>
    </>
  );
}

/** The short name that starts a dungeon's row, the full one on hover. The
 * tiles already name it, so screen readers skip it. */
function RowLabel({ row, ink }: { row: DungeonRow; ink: string }) {
  return (
    <Tip tip={row.name}>
      <span
        aria-hidden
        className={cn(
          "self-center pr-1 font-semibold text-[10.5px] leading-3.5 tracking-[0.04em] opacity-80",
          ink,
        )}
      >
        {row.short}
      </span>
    </Tip>
  );
}
