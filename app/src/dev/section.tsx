import type { ReactNode } from "react";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { cn } from "@/lib/utils";

/** What the section's last request said, as a status, or as an alert when
 * it failed. */
export function Said() {
  const { said } = useRequest();
  if (!said) return null;
  return (
    <p
      role={said.ok ? "status" : "alert"}
      className={cn(
        "mt-1.5 whitespace-pre-line text-xs",
        said.ok ? "text-ink-muted" : "text-danger-ink",
      )}
    >
      {said.text}
    </p>
  );
}

/** A titled part of the dev window with its own request: one at a time,
 * what it said shown under it. Call `useRequest` in its children, not
 * above it. */
export function DevSection({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <section aria-label={title} className="border-line border-t px-3 py-2.5">
      <h2 className="mb-1.5 font-semibold text-ink-muted text-xs uppercase tracking-wide">
        {title}
      </h2>
      <RequestProvider>
        {children}
        <Said />
      </RequestProvider>
    </section>
  );
}
