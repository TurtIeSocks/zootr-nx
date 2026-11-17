import { afterEach, describe, expect, it, vi } from "vitest";
import {
  fetchLatest,
  newer,
  notesHtml,
  stripChangelog,
} from "@/update/release";

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("newer", () => {
  it.each([
    ["0.1.0", "0.2.0", true],
    ["0.9.0", "0.10.0", true],
    ["1.2.3", "1.2.3", false],
    ["0.2.0", "0.1.9", false],
    ["0.1.0", "v0.1.1", true],
    ["0.1.0", "v0.2.0-beta", false],
    ["0.1.0", "latest", false],
  ])("%s then %s: %s", (current, next, is) => {
    expect(newer(current, next)).toBe(is);
  });
});

describe("notesHtml", () => {
  it("keeps headings, lists and links", () => {
    const html = notesHtml(
      "## What's Changed\n\n* feat: marks in [#12](https://github.com/o/r/pull/12)",
    );
    expect(html).toContain("<h2>What's Changed</h2>");
    expect(html).toContain("<li>");
    expect(html).toContain('href="https://github.com/o/r/pull/12"');
  });

  it("strips script, handlers and javascript links", () => {
    const html = notesHtml(
      '* <img src=x onerror="alert(1)"> by @x\n\n<script>alert(1)</script>\n\n[click](javascript:alert(1))',
    );
    expect(html).not.toContain("onerror");
    expect(html).not.toContain("<script");
    expect(html).not.toContain("javascript:");
  });
});

describe("stripChangelog", () => {
  it("drops the link line in either spelling", () => {
    const tail = "https://github.com/o/r/compare/v0.1.0...v0.2.0";
    expect(stripChangelog(`- a fix\n\nFull changelog: ${tail}`)).toBe(
      "- a fix",
    );
    expect(stripChangelog(`- a fix\n\n**Full Changelog**: ${tail}\n`)).toBe(
      "- a fix",
    );
  });

  it("keeps a line that only mentions the changelog", () => {
    const md = "- the full changelog moved to CHANGELOG.md";
    expect(stripChangelog(md)).toBe(md);
  });
});

describe("fetchLatest", () => {
  const answer = (res: unknown) =>
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue(res));
  const quiet = () => vi.spyOn(console, "debug").mockImplementation(() => {});

  it("maps the release", async () => {
    answer({
      ok: true,
      json: async () => ({
        tag_name: "v0.2.0",
        html_url: "https://github.com/o/r/releases/tag/v0.2.0",
        body: "notes",
        published_at: "2026-11-14T00:00:00Z",
      }),
    });
    expect(await fetchLatest("o/r")).toEqual({
      version: "0.2.0",
      url: "https://github.com/o/r/releases/tag/v0.2.0",
      changelog: "https://github.com/o/r/blob/main/CHANGELOG.md",
      notes: "notes",
      published: "2026-11-14T00:00:00Z",
    });
    expect(fetch).toHaveBeenCalledWith(
      "https://api.github.com/repos/o/r/releases/latest",
      expect.anything(),
    );
  });

  it("gives null for a 404", async () => {
    quiet();
    answer({ ok: false, status: 404, json: async () => ({}) });
    expect(await fetchLatest("o/r")).toBeNull();
  });

  it("gives null when offline", async () => {
    quiet();
    vi.stubGlobal("fetch", vi.fn().mockRejectedValue(new TypeError("offline")));
    expect(await fetchLatest("o/r")).toBeNull();
  });

  it("gives null for a body that isn't a release", async () => {
    quiet();
    answer({ ok: true, json: async () => ({ message: "rate limited" }) });
    expect(await fetchLatest("o/r")).toBeNull();
  });
});
