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
 * runs into the folder body, the rest tuck behind it. Arrow keys move and
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
      <Tabs.List
        aria-label={label}
        className="relative z-2 flex items-end gap-1"
      >
        {tabs.map(({ id, label, icon: Icon, meta }) => (
          <Tabs.Trigger
            key={id}
            value={id}
            className="group/tab -mb-1 relative inline-flex h-9 cursor-pointer items-center gap-2 whitespace-nowrap rounded-t-lg border border-line border-b-0 bg-surface-sunken px-4 pb-1 font-medium text-[13.5px] text-ink-muted transition-[background-color,color,transform,height] duration-(--duration-base) ease-out hover:-translate-y-0.5 hover:bg-surface-hover hover:text-ink focus-visible:outline-offset-[-2px] data-[state=active]:z-3 data-[state=active]:-mb-px data-[state=active]:h-[42px] data-[state=active]:translate-y-0 data-[state=active]:bg-surface data-[state=active]:pb-0 data-[state=active]:text-ink data-[state=active]:shadow-[0_-1px_3px_rgba(0,0,0,0.04)]"
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
