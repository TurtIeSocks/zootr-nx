import { openUrl } from "@tauri-apps/plugin-opener";
import { ArrowUpRight } from "lucide-react";
import { type MouseEvent, useMemo } from "react";
import { cn } from "@/lib/utils";
import { notesHtml } from "@/update/release";

/** Markdown's elements, styled for release notes. */
const NOTES =
  "[&>:first-child]:mt-0 text-ink-muted text-sm leading-relaxed [&_a]:text-primary-ink [&_a]:underline [&_code]:font-mono [&_code]:text-[12px] [&_:is(h1,h2,h3,h4,h5,h6)]:mt-2 [&_:is(h1,h2,h3,h4,h5,h6)]:font-medium [&_:is(h1,h2,h3,h4,h5,h6)]:text-ink [&_li+li]:mt-1.5 [&_ol]:list-decimal [&_ol]:pl-5 [&_p]:mt-1.5 [&_ul]:list-disc [&_ul]:pl-5";

/** Opens a link from the notes in the browser, never in the webview. */
function follow(e: MouseEvent) {
  const a = (e.target as Element).closest("a");
  if (!a) return;
  e.preventDefault();
  if (a.protocol === "https:") void openUrl(a.href);
}

/** Props for {@link Notes}. */
export interface NotesProps {
  md: string;
  /** Its frame: padding, and a height to scroll in. */
  className?: string;
}

/** A release's notes, sanitized by {@link notesHtml}. */
export function Notes({ md, className }: NotesProps) {
  const html = useMemo(() => notesHtml(md), [md]);
  if (!html.trim()) return null;
  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: clicks bubble up from the links inside, which take focus and keys themselves.
    // biome-ignore lint/a11y/useKeyWithClickEvents: as above.
    <div
      className={cn(NOTES, className)}
      onClick={follow}
      // biome-ignore lint/security/noDangerouslySetInnerHtml: notesHtml sanitizes with DOMPurify.
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

/** Props for {@link ReleaseLink}. */
export interface ReleaseLinkProps {
  href: string;
  children: string;
}

/** A link beside a release's name, opened in the browser. */
export function ReleaseLink({ href, children }: ReleaseLinkProps) {
  return (
    <a
      href={href}
      className="inline-flex items-center gap-0.5 hover:text-ink hover:underline"
      onClick={(e) => {
        e.preventDefault();
        void openUrl(href);
      }}
    >
      {children}
      <ArrowUpRight className="size-3" />
    </a>
  );
}
