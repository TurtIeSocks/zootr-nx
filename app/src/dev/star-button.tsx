import { Star } from "lucide-react";
import { HOVER_ACTION } from "@/dev/styles";
import { cn } from "@/lib/utils";

/** Stars or unstars `name`: a row's hover action, or always shown with
 * `shown`. Gold once starred. */
export function StarButton({
  name,
  on,
  onToggle,
  shown,
  disabled,
}: {
  name: string;
  on: boolean;
  onToggle: () => void;
  shown?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={`${on ? "Unstar" : "Star"} ${name}`}
      aria-pressed={on}
      disabled={disabled}
      onClick={onToggle}
      className={cn(
        HOVER_ACTION,
        (on || shown) && "opacity-100",
        on ? "text-gold [&_svg]:fill-gold" : "text-ink-faint",
      )}
    >
      <Star />
    </button>
  );
}
