import type { LucideIcon } from "lucide-react";
import { Tabs } from "radix-ui";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/** One tab of {@link FolderTabs} and the folder it opens. */
export interface FolderTab<T extends string> {
  id: T;
  label: string;
  icon: LucideIcon;
  /** A mark after the label: a dot, a count. */
  meta?: ReactNode;
  panel: ReactNode;
}

/** Props for {@link FolderTabs}. */
interface FolderTabsProps<T extends string> {
  tabs: FolderTab<T>[];
  value: T;
  onValueChange: (id: T) => void;
  /** The tab list's name. */
  label: string;
}

/** Tabs drawn like the labels on files in a cabinet drawer: the open one
 * runs into the folder body (above it), the rest tuck behind its top edge
 * (below it). The list sets no z-index of its own, so each tab stacks
 * against the body. Only colors animate: a switch doesn't move anything. Arrow keys move and
 * open, Home and End jump. */
export function FolderTabs<T extends string>({
  tabs,
  value,
  onValueChange,
  label,
}: FolderTabsProps<T>) {
  return (
    <Tabs.Root
      value={value}
      onValueChange={(id) => onValueChange(id as T)}
      className="flex flex-col"
    >
      <Tabs.List aria-label={label} className="flex items-end gap-1">
        {tabs.map(({ id, label, icon: Icon, meta }) => (
          <Tabs.Trigger
            key={id}
            value={id}
            className="group/tab -mb-1 relative z-0 inline-flex h-9 cursor-pointer items-center gap-2 whitespace-nowrap rounded-t-lg border border-line border-b-0 bg-surface-sunken px-4 pb-1 font-medium text-[13.5px] text-ink-muted transition-colors duration-(--duration-fast) hover:bg-surface-hover hover:text-ink focus-visible:outline-offset-[-2px] data-[state=active]:z-2 data-[state=active]:-mb-px data-[state=active]:h-[42px] data-[state=active]:bg-surface data-[state=active]:pb-0 data-[state=active]:text-ink"
          >
            <Icon className="size-4 stroke-[1.75] text-ink-faint group-data-[state=active]/tab:text-primary-ink" />
            {label}
            {meta}
          </Tabs.Trigger>
        ))}
      </Tabs.List>
      {tabs.map(({ id, panel }) => (
        <Tabs.Content
          key={id}
          value={id}
          className={cn(
            "relative z-1 rounded-lg border border-line bg-surface shadow-xs",
            id === tabs[0]?.id && "rounded-tl-none",
          )}
        >
          {panel}
        </Tabs.Content>
      ))}
    </Tabs.Root>
  );
}
