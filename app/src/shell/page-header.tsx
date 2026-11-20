import { ChevronRight } from "lucide-react";
import { Fragment, type ReactNode } from "react";
import { Help } from "@/components/help";
import type { HelpTopic } from "@/help/topics";
import { cn } from "@/lib/utils";
import { crumbs } from "@/shell/nav";
import { useUi } from "@/stores/ui";

/** The content's drag band, as tall as the title bar, with the breadcrumb. */
export function Topbar() {
  const [tab] = useUi.usePath("tab");
  const [section] = useUi.usePath("section");
  const trail = crumbs(tab, section);
  return (
    <div
      data-tauri-drag-region
      className="flex h-(--titlebar) shrink-0 items-center gap-2 pr-4 pl-8"
    >
      <p
        data-tauri-drag-region
        className="flex items-center gap-1.5 text-[13px] text-ink-faint"
      >
        {trail.map((c, i) => (
          <Fragment key={c}>
            {i > 0 && <ChevronRight aria-hidden className="size-3.5" />}
            <span
              className={cn(i === trail.length - 1 && "font-medium text-ink")}
            >
              {c}
            </span>
          </Fragment>
        ))}
      </p>
    </div>
  );
}

/** Props for {@link PageHeader}. */
interface PageHeaderProps {
  title: string;
  /** A help bubble beside the title. */
  help?: HelpTopic;
  description?: ReactNode;
  actions?: ReactNode;
}

/** A page's title, its one-line description and actions at the trailing
 * edge. */
export function PageHeader({
  title,
  help,
  description,
  actions,
}: PageHeaderProps) {
  return (
    <header className="flex items-end justify-between gap-6 pt-2 pb-1">
      <div className="min-w-0">
        <div className="flex items-center gap-2">
          <h1 className="type-display text-ink">{title}</h1>
          {help && <Help topic={help} />}
        </div>
        {description && (
          <p className="mt-1 max-w-[60ch] text-[13px] text-ink-muted leading-[18px]">
            {description}
          </p>
        )}
      </div>
      {actions && <div className="flex shrink-0 gap-2">{actions}</div>}
    </header>
  );
}

/** Props for {@link Page}. */
interface PageProps {
  wide?: boolean;
  className?: string;
  children: ReactNode;
}

/** The centred page column. `wide` lets tables and logs run full width. */
export function Page({ wide, className, children }: PageProps) {
  return (
    <div
      className={cn(
        "mx-auto flex w-full flex-col gap-5",
        !wide && "max-w-(--content-width)",
        className,
      )}
    >
      {children}
    </div>
  );
}
