import {
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  ChevronUp,
  EyeOff,
  type LucideIcon,
  Minus,
  Plus,
} from "lucide-react";
import type { Placement, Section } from "@/bindings";
import { Button } from "@/components/ui/button";
import { type Move, move, TITLES } from "@/items/layout";
import { setSections, useItems } from "@/stores/items";

/** The four arrows, in the toolbar's order. `{}` is the section's title. */
const ARROWS: [Move, LucideIcon, string][] = [
  ["left", ChevronLeft, "Move {} left"],
  ["up", ChevronUp, "Move {} up"],
  ["down", ChevronDown, "Move {} down"],
  ["right", ChevronRight, "Move {} right"],
];

/** A thin rule between the toolbar's groups. */
function Divider() {
  return <span aria-hidden className="mx-0.5 h-4 w-px bg-line" />;
}

/** Props for {@link SectionToolbar}. */
interface SectionToolbarProps {
  placement: Placement;
  /** How many tiles the section holds, which bounds its width. */
  slots: number;
  /** Keeps its width, so no stepper. */
  fixed?: boolean;
}

/** Floats above a section in the Items tab. Shown on hover or
 * focus, so the preview matches the pop out at rest. */
export function SectionToolbar({
  placement: { section, width },
  slots,
  fixed,
}: SectionToolbarProps) {
  const title = TITLES[section];
  const at = { section, slots };
  return (
    <div
      role="toolbar"
      aria-label={`${title} layout`}
      className="absolute bottom-full left-0 z-10 mb-1.5 flex items-center gap-0.5 whitespace-nowrap rounded-sm border border-line bg-surface-raised p-0.5 text-ink opacity-0 shadow-md transition-opacity duration-(--duration-fast) focus-within:opacity-100 group-hover:opacity-100"
    >
      {ARROWS.map(([m, icon, label]) => (
        <MoveButton key={m} {...at} move={m} icon={icon} label={label} />
      ))}
      <Divider />
      {!fixed && (
        <>
          <MoveButton
            {...at}
            move="narrower"
            icon={Minus}
            label="Narrower {}"
          />
          <span className="w-5 text-center text-ink-muted text-xs tabular-nums">
            {width}
          </span>
          <MoveButton {...at} move="wider" icon={Plus} label="Wider {}" />
          <Divider />
        </>
      )}
      <MoveButton {...at} move="hide" icon={EyeOff} label="Hide {}" />
    </div>
  );
}

/** Props for {@link MoveButton}. */
interface MoveButtonProps {
  section: Section;
  slots: number;
  move: Move;
  icon: LucideIcon;
  /** Its label, `{}` standing for the section's title. */
  label: string;
}

/** Moves or resizes the section, disabled where it can't go. */
function MoveButton({
  section,
  slots,
  move: m,
  icon: Icon,
  label,
}: MoveButtonProps) {
  const [sections] = useItems.usePath("options.sections");
  const next = move(sections, section, m, slots);
  return (
    <Button
      size="icon-xs"
      variant="ghost"
      aria-label={label.replace("{}", TITLES[section])}
      disabled={!next}
      onClick={() => next && setSections(next)}
    >
      <Icon />
    </Button>
  );
}
