import { cn } from "@/lib/utils";

/** Props for {@link Marked}. */
interface MarkedProps {
  text: string;
  /** Lower-case words, each bolded wherever it appears. */
  words: string[];
  className?: string;
}

/** Which characters of `text` any of `words` covers. An empty word covers
 * none. Indices come from `text`'s lower case, so a character whose lower
 * case is longer (like `İ`) shifts the marks after it. */
function covered(text: string, words: string[]): boolean[] {
  const lower = text.toLowerCase();
  const hit = new Array<boolean>(text.length).fill(false);
  for (const w of words) {
    if (!w) continue;
    for (let at = lower.indexOf(w); at >= 0; at = lower.indexOf(w, at + 1))
      hit.fill(true, at, at + w.length);
  }
  return hit;
}

/** `text` with every match of every word bolded. */
export function Marked({ text, words, className }: MarkedProps) {
  const hit = covered(text, words);
  const runs: { text: string; bold: boolean }[] = [];
  // By UTF-16 unit, as `indexOf` counts.
  for (let i = 0; i < text.length; i++) {
    const bold = hit[i] ?? false;
    const last = runs.at(-1);
    if (last?.bold === bold) last.text += text[i];
    else runs.push({ text: text[i] ?? "", bold });
  }
  return (
    <span className={cn("truncate", className)}>
      {runs.map((r, i) =>
        r.bold ? (
          // biome-ignore lint/suspicious/noArrayIndexKey: runs are fixed for a given text and words
          <b key={i} className="font-semibold text-ink">
            {r.text}
          </b>
        ) : (
          r.text
        ),
      )}
    </span>
  );
}
