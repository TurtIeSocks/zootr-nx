import { Star } from "lucide-react";
import { iconUrl } from "@/items/icons";
import { cn } from "@/lib/utils";
import { useItems } from "@/stores/items";

/** Props for {@link ItemIcon}. */
interface ItemIconProps {
  /** The item's actor name. */
  item: string;
}

/** The item's icon from the tracker's set, once the dump's are built.
 * Items it has none for show none. */
export function ItemIcon({ item }: ItemIconProps) {
  const game = useItems((s) => s.art === "game");
  if (!game) return null;
  return (
    <img
      src={iconUrl(item)}
      alt=""
      className="size-5 shrink-0 object-contain"
      onError={(e) => {
        e.currentTarget.hidden = true;
      }}
    />
  );
}

/** Props for {@link HeroStar}. */
interface HeroStarProps {
  className?: string;
}

/** ★ for a check on the Way of the Hero. */
export function HeroStar({ className }: HeroStarProps) {
  return (
    <Star
      aria-label="Way of the Hero"
      className={cn("size-3.5 shrink-0 fill-gold text-gold", className)}
    />
  );
}
