import { openTab, PANEL, TABS, tabId } from "@/dev/nav";
import { cn } from "@/lib/utils";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

/** The underlined strip of tabs, icon over label. Arrow keys move along
 * it. */
export function Tabs() {
  const [open] = useUi.usePath("dev.tab");

  const onKeyDown = (e: React.KeyboardEvent) => {
    const step = { ArrowRight: 1, ArrowLeft: -1 }[e.key];
    if (!step) return;
    e.preventDefault();
    const at = TABS.findIndex((t) => t.id === open);
    const next = TABS[(at + step + TABS.length) % TABS.length];
    if (!next) return;
    openTab(next.id);
    document.getElementById(tabId(next.id))?.focus();
  };

  return (
    <div
      role="tablist"
      aria-label="Dev tools"
      onKeyDown={onKeyDown}
      className="flex border-line border-b bg-surface px-2"
    >
      {TABS.map((t) => (
        <Tab key={t.id} {...t} selected={t.id === open} />
      ))}
    </div>
  );
}

function Tab({
  id,
  label,
  icon: Icon,
  selected,
}: (typeof TABS)[number] & { selected: boolean }) {
  const [hidden] = useDev.usePath("live.hidden");
  const count = id === "actors" ? hidden.length : 0;

  return (
    <button
      type="button"
      role="tab"
      aria-label={count ? `${label}, ${count} hidden` : undefined}
      id={tabId(id)}
      aria-selected={selected}
      aria-controls={PANEL}
      tabIndex={selected ? 0 : -1}
      onClick={() => openTab(id)}
      className={cn(
        "relative flex flex-1 cursor-pointer flex-col items-center gap-0.5 pt-1 pb-2 font-medium text-[12px] text-ink-muted transition-colors duration-(--duration-fast) hover:text-ink",
        selected &&
          "text-ink after:absolute after:inset-x-3 after:bottom-0 after:h-0.5 after:rounded-full after:bg-primary",
      )}
    >
      <span className="relative">
        <Icon
          aria-hidden
          className={cn("size-4.5", selected && "text-primary-ink")}
        />
        {count > 0 && (
          <span
            aria-hidden
            className="absolute -top-1.5 left-3.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-surface-sunken px-1 text-[10px] text-ink-muted tabular-nums"
          >
            {count}
          </span>
        )}
      </span>
      {label}
    </button>
  );
}
