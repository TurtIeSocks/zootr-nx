import { useMemo } from "react";
import { type Cell, type Placement, TRACKER_COLUMNS } from "@/bindings";
import { isLight } from "@/items/fallback-icons";
import { columnsOf, inSection, PIXELS, TITLES } from "@/items/layout";
import { SectionToolbar } from "@/items/section-toolbar";
import { SectionGrid, Tile } from "@/items/tile";
import { useGained } from "@/items/use-gained";
import { cn } from "@/lib/utils";
import { useItems } from "@/stores/items";

/** Ink for what's drawn straight on the tracker's background: dark on a
 * light one, white on a dark one, whichever contrasts more. */
function useInk() {
  const light = useItems((s) => isLight(s.options.background));
  return light ? "text-neutral-900" : "text-white";
}

/** Props for {@link TrackerTile}. */
interface TrackerTileProps {
  cell: Cell;
  size: number;
  ink: string;
}

/** A tile that flashes when its tier goes up. */
function TrackerTile({ cell, size, ink }: TrackerTileProps) {
  const { gained, done } = useGained(cell.tier);
  return (
    <Tile
      cell={cell}
      size={size}
      ink={ink}
      className={cn(gained && "animate-gain")}
      onAnimationEnd={done}
    />
  );
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
  const columns = columnsOf(sections, TRACKER_COLUMNS);
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

/** A section of the tracker, with its layout toolbar while editing. */
function GridSection({ placement, edit }: GridSectionProps) {
  const { section } = placement;
  const all = useItems((s) => s.cells);
  const cells = useMemo(() => inSection(all, section), [all, section]);
  const size = useItems((s) => PIXELS[s.options.size]);
  const ink = useInk();
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
          fixed={cells.some(({ cell }) => cell.row)}
        />
      )}
      <SectionGrid placement={placement} cells={cells} size={size} ink={ink}>
        {(cell) => <TrackerTile cell={cell} size={size} ink={ink} />}
      </SectionGrid>
    </section>
  );
}
