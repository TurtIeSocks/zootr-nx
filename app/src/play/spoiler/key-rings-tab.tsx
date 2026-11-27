import { useSpoiler } from "@/play/spoiler/spoiler-context";

/** Each dungeon whose small keys come as one ring. Every other dungeon
 * takes its keys one at a time. */
export function KeyRingsTab() {
  const { spoiler } = useSpoiler();
  return (
    <div className="flex flex-col gap-2 px-5 py-4">
      <p className="text-[13px] text-ink-muted">
        These dungeons&rsquo; small keys come as one ring. The rest take theirs
        one at a time.
      </p>
      <ul aria-label="Key rings" className="flex flex-col">
        {spoiler.key_rings.map((dungeon) => (
          <li
            key={dungeon}
            className="flex h-8.5 items-center px-2 text-[13px] font-medium"
          >
            {dungeon}
          </li>
        ))}
      </ul>
    </div>
  );
}
