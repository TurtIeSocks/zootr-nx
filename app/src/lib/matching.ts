/** The words of `query`, lower case. */
export const words = (query: string) =>
  query.toLowerCase().split(/\s+/).filter(Boolean);

/** Whether `text` holds every one of `ws`, any case, in any order. */
export const hasAll = (text: string, ws: string[]) => {
  const lower = text.toLowerCase();
  return ws.every((w) => lower.includes(w));
};

/** The first `limit` of `names` holding every word of the query, any case
 * and any order. */
export function matching(names: string[], query: string, limit = 50) {
  const ws = words(query);
  if (ws.length === 0) return [];
  const out: string[] = [];
  for (const name of names) {
    if (hasAll(name, ws)) out.push(name);
    if (out.length === limit) break;
  }
  return out;
}
