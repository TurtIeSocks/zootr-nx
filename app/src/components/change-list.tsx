import { ArrowRight } from "lucide-react";
import type { Change } from "@/lib/settings";
import { useConfig } from "@/stores/config";

/** Props for {@link ChangeList}. */
interface ChangeListProps {
  changes: Change[];
}

/** Settings that differ, under their groups in schema order, each as
 * "Name  old → new" with the old value struck through. */
export function ChangeList({ changes }: ChangeListProps) {
  const [groups] = useConfig.usePath("groups");
  return (
    <ul className="flex flex-col gap-0.5 text-[12.5px]">
      {groups.flatMap(({ name }) => {
        const mine = changes.filter((c) => c.def.group === name);
        if (mine.length === 0) return [];
        return [
          <li key={`group:${name}`} className="type-overline mt-1.5">
            {name}
          </li>,
          ...mine.map(({ def, before, after }) => (
            <li
              key={def.id}
              className="grid grid-cols-[1fr_auto_14px_auto] items-center gap-1.5 py-[3px]"
            >
              <span>{def.label}</span>
              <s className="text-ink-muted decoration-ink-faint">{before}</s>
              <ArrowRight aria-label="to" className="size-3 text-ink-faint" />
              <span className="font-medium text-ink">{after}</span>
            </li>
          )),
        ];
      })}
    </ul>
  );
}
