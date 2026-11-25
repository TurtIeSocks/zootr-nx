import { Star } from "lucide-react";
import { Button } from "@/components/ui/button";
import { StarButton } from "@/dev/star-button";
import { useStars } from "@/dev/stars";
import type { DevStars } from "@/stores/ui";

/** Name kinds a field stars. */
type Kind = Exclude<keyof DevStars, "actors">;

/** What an empty field's star says it would star. */
const NOUN: Record<Kind, string> = {
  items: "an item",
  quests: "a quest",
  values: "a GameData name",
};

/** The star after a name field: stars what's typed once the list has it,
 * and unstars a starred one. */
export function FieldStar({
  kind,
  value,
  names,
}: {
  kind: Kind;
  value: string;
  names: string[];
}) {
  const stars = useStars(kind);
  const name = value.trim();
  const on = stars.has(name);
  return (
    <div className="flex h-8 shrink-0 items-center [&_svg]:size-3.5">
      <StarButton
        shown
        name={name || NOUN[kind]}
        on={on}
        disabled={!on && !names.includes(name)}
        onToggle={() => stars.toggle(name)}
      />
    </div>
  );
}

/** The starred names of `kind`, each filling its field with a click. */
export function StarredNames({
  kind,
  onPick,
}: {
  kind: Kind;
  onPick: (name: string) => void;
}) {
  const { list } = useStars(kind);
  if (list.length === 0) return null;

  return (
    <div className="mt-2.5 flex flex-wrap items-center gap-1.5">
      <span className="flex items-center gap-1 text-ink-faint text-xs">
        <Star className="size-3 fill-gold text-gold" /> Starred
      </span>
      {list.map((name) => (
        <Button
          key={name}
          size="sm"
          variant="secondary"
          aria-label={`Fill ${name}`}
          onClick={() => onPick(name)}
        >
          {name}
        </Button>
      ))}
    </div>
  );
}
