import { Segmented } from "@/components/ui/segmented";
import { useUi } from "@/stores/ui";

const ORDERS = [
  { id: "az", label: "A–Z" },
  { id: "near", label: "Nearest" },
] as const;

/** Picks one actor list's order, kept for that list alone. */
export function SortToggle({ list }: { list: "warp" | "all" | "nearby" }) {
  const [sort, setSort] = useUi.usePath(`dev.sort.${list}`);
  return (
    <Segmented
      aria-label="Sort"
      className="shrink-0"
      options={ORDERS}
      value={sort}
      onValueChange={setSort}
    />
  );
}
