/** The first `limit` of `names` containing the query, any case. */
export function matching(names: string[], query: string, limit = 50) {
  const q = query.trim().toLowerCase();
  if (!q) return [];
  const out: string[] = [];
  for (const name of names) {
    if (name.toLowerCase().includes(q)) out.push(name);
    if (out.length === limit) break;
  }
  return out;
}
