import DOMPurify from "dompurify";
import { marked } from "marked";

/** The repo whose releases the app watches. */
export const RELEASES_REPO = "TurtIeSocks/zootr-nx";

/** This version's `znx.apworld`, which the release attaches for hosts, or
 * the latest release's while the version is unknown. */
export const apworldUrl = (version: string | null) =>
  `https://github.com/${RELEASES_REPO}/releases/${version ? `download/v${version}` : "latest/download"}/znx.apworld`;

/** The Archipelago setup guide, joining and hosting. */
export const AP_GUIDE_URL = `https://github.com/${RELEASES_REPO}/blob/main/ap/znx/docs/setup_en.md`;

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

/** GitHub's release `r` of `repo` as the app shows it. Throws on
 * anything that isn't one. */
function toRelease(repo: string, r: unknown): Release {
  const o = r as Record<string, unknown> | null;
  if (typeof o?.tag_name !== "string" || typeof o?.html_url !== "string")
    throw new Error("Not a release");
  return {
    version: o.tag_name.replace(/^v/, ""),
    url: o.html_url,
    changelog: `https://github.com/${repo}/blob/main/CHANGELOG.md`,
    notes: typeof o.body === "string" ? stripChangelog(o.body) : "",
    published: typeof o.published_at === "string" ? o.published_at : "",
  };
}

/** GitHub's answer at `path` of `repo`'s API, as JSON. Throws when it
 * fails. */
async function ask(repo: string, path: string): Promise<unknown> {
  const res = await fetch(`https://api.github.com/repos/${repo}/${path}`, {
    headers: { Accept: "application/vnd.github+json" },
  });
  if (!res.ok) throw new Error(`GitHub answered ${res.status}`);
  return res.json();
}

/** The latest published release of `repo`, or null when there's none or
 * the check fails. Drafts and prereleases never count. */
export async function fetchLatest(repo: string): Promise<Release | null> {
  try {
    return toRelease(repo, await ask(repo, "releases/latest"));
  } catch (e) {
    console.debug("Couldn't check for updates", e);
    return null;
  }
}

/** How many releases one page of the changelog holds. */
export const PAGE = 10;

/** One page of the changelog. */
export interface ReleasePage {
  /** Its published releases, newest first. */
  releases: Release[];
  /** Whether GitHub gave a full page, so an older one may follow. */
  more: boolean;
}

/** Page `page` (from 1) of `repo`'s releases. Drafts and prereleases are
 * left out, after `more` counts them. Throws when the request fails. */
export async function fetchReleases(
  repo: string,
  page: number,
): Promise<ReleasePage> {
  const list = await ask(repo, `releases?per_page=${PAGE}&page=${page}`);
  if (!Array.isArray(list)) throw new Error("Not a list of releases");
  return {
    releases: list
      .filter((r) => !r?.draft && !r?.prerelease)
      .map((r) => toRelease(repo, r)),
    more: list.length === PAGE,
  };
}

/** An ISO 8601 date, as "Nov 21, 2026". */
export const day = (iso: string) =>
  new Date(iso).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    year: "numeric",
  });

/** Release notes as safe HTML. PR titles land in generated notes, and the
 * webview can call every app command, so this is the only way release
 * text reaches the page. */
export const notesHtml = (md: string) =>
  DOMPurify.sanitize(marked.parse(md, { async: false }));
