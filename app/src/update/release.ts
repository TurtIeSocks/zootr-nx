import DOMPurify from "dompurify";
import { marked } from "marked";

/** The repo whose releases the app watches. */
export const RELEASES_REPO = "TurtIeSocks/zootr-nx";

/** A published release, as the update notice shows it. */
export interface Release {
  /** The tag less its `v`. */
  version: string;
  /** The release page. */
  url: string;
  /** The repo's CHANGELOG.md. */
  changelog: string;
  /** Markdown. Untrusted: render it only through {@link notesHtml}. */
  notes: string;
  /** ISO 8601, or "" when GitHub gave none. */
  published: string;
}

const parts = (v: string) =>
  /^v?(\d+)\.(\d+)\.(\d+)$/.exec(v.trim())?.slice(1).map(Number);

/** Whether version `next` is newer than `current`, by major.minor.patch. A
 * version that doesn't parse never is. */
export function newer(current: string, next: string): boolean {
  const [a, b] = [parts(current), parts(next)];
  if (!a || !b) return false;
  const i = a.findIndex((n, k) => n !== b[k]);
  return i >= 0 && (b[i] ?? 0) > (a[i] ?? 0);
}

/** A "Full changelog: URL" line, as GitHub's generated notes end. */
const CHANGELOG_LINE = /^\s*\**full changelog\**:?\**\s*\S+\s*$/gim;

/** `md` without its "Full changelog" link line: the notice has a
 * Changelog button instead. */
export const stripChangelog = (md: string) =>
  md.replace(CHANGELOG_LINE, "").trim();

/** The latest published release of `repo`, or null when there's none or
 * the check fails. Drafts and prereleases never count. */
export async function fetchLatest(repo: string): Promise<Release | null> {
  try {
    const res = await fetch(
      `https://api.github.com/repos/${repo}/releases/latest`,
      { headers: { Accept: "application/vnd.github+json" } },
    );
    if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
    const r = await res.json();
    if (typeof r?.tag_name !== "string" || typeof r?.html_url !== "string")
      throw new Error("Not a release");
    return {
      version: r.tag_name.replace(/^v/, ""),
      url: r.html_url,
      changelog: `https://github.com/${repo}/blob/main/CHANGELOG.md`,
      notes: typeof r.body === "string" ? stripChangelog(r.body) : "",
      published: typeof r.published_at === "string" ? r.published_at : "",
    };
  } catch (e) {
    console.debug("Couldn't check for updates", e);
    return null;
  }
}

/** Release notes as safe HTML. PR titles land in generated notes, and the
 * webview can call every app command, so this is the only way release
 * text reaches the page. */
export const notesHtml = (md: string) =>
  DOMPurify.sanitize(marked.parse(md, { async: false }));
