import { doorName } from "@/play/spoiler/rows";
import { useSpoiler } from "@/play/spoiler/spoiler-context";

/** Each shuffled dungeon door and the dungeon behind it. */
export function EntrancesTab() {
  const { spoiler } = useSpoiler();
  return (
    <ul aria-label="Entrances" className="flex flex-col px-5 py-4">
      {spoiler.entrances.map((d) => (
        <li
          key={d.id}
          className="flex h-8.5 items-center gap-2 px-2 text-[13px]"
        >
          <span className="w-80 text-ink-muted">{doorName(d)}</span>
          <span className="flex-1 font-medium">{d.dungeon_name}</span>
        </li>
      ))}
    </ul>
  );
}
