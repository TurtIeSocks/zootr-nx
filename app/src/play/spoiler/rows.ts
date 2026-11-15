import type { Held, Spoiler, Spot } from "@/bindings";

/** The spoiler's views. */
export type SpoilerTab = "spheres" | "hero" | "checks";

/** A spot's area by name. History fills the names in for every seed; the
 * ids only satisfy types. */
export const areaOf = (s: Spot) => s.area_name || s.area;
/** A spot's place by name, or its chest's, or its vanilla id. */
export const placeOf = (s: Spot) => s.place || s.chest_name || s.vanilla;
/** What a spot holds, by name. */
export const itemOf = (s: Spot) => s.item_name || s.item;

/** Whether the seed kept its rounds, Way of the Hero and rules. Seeds from
 * before them, and seeds logic didn't place, have none. */
export const hasLogic = ({ spots, events }: Spoiler) =>
  events.length > 0 || spots.some((s) => s.round != null || s.hero || s.need);

/** A check, or an item Link gets outside the shuffle. */
export type Card =
  | { kind: "check"; spot: Spot }
  | { kind: "source"; name: string };

/** A round and what it collects. */
export interface Column {
  round: number;
  cards: Card[];
}

/** A column a round, its checks by area then its sources. `heroOnly` keeps
 * the Way of the Hero's checks and the rounds that hold one. */
export function columns({ spots, events }: Spoiler, heroOnly: boolean) {
  const rounds = [
    ...spots.map((s) => s.round ?? 0),
    ...events.map((e) => e.round),
  ];
  const last = Math.max(0, ...rounds);
  const cards = (round: number): Card[] => [
    ...spots
      .filter((s) => s.round === round && (!heroOnly || s.hero))
      .map((spot) => ({ kind: "check" as const, spot })),
    ...events
      .filter((e) => !heroOnly && e.round === round)
      .map((e) => ({ kind: "source" as const, name: e.name })),
  ];
  return Array.from({ length: last }, (_, i) => ({
    round: i + 1,
    cards: cards(i + 1),
  })).filter((c) => c.cards.length > 0);
}

/** An area and its spots. */
export interface Area {
  name: string;
  spots: Spot[];
}

/** The Way of the Hero's checks by area, in the seed's order. */
export function heroAreas(spots: Spot[]): Area[] {
  const groups = new Map<string, Spot[]>();
  for (const s of spots.filter((s) => s.hero))
    groups.set(areaOf(s), [...(groups.get(areaOf(s)) ?? []), s]);
  return [...groups].map(([name, spots]) => ({ name, spots }));
}

/** The spots whose area, place or item holds `query`. */
export function search(spots: Spot[], query: string) {
  const q = query.toLowerCase();
  return spots.filter((s) =>
    [areaOf(s), placeOf(s), itemOf(s)].some((f) => f.toLowerCase().includes(q)),
  );
}

/** A piece of a rule: text, or an item with the check giving it soonest. */
/** A piece of a rule: plain text, or an item it names, with the spot that
 * gives it when there is one. */
export type Part =
  | { text: string }
  | { item: string; name: string; giver?: Spot };

/** `s` escaped to match itself in a regex. */
const literal = (s: string) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** `Item_Gauntlet_Goron` as "Gauntlet Goron". */
const readable = (id: string) => id.replace(/^Item_/, "").replaceAll("_", " ");

/** Orders spots by round, the ones without one last. */
const soonest = (a: Spot, b: Spot) =>
  (a.round ?? Number.POSITIVE_INFINITY) - (b.round ?? Number.POSITIVE_INFINITY);

/** The item `id` as a rule part: held at the start, given by its soonest
 * spot, or named only. */
function item(id: string, spots: Spot[], start: Held[]): Part {
  const held = start.find((h) => h.item === id);
  if (held) return { item: id, name: `${held.name} (start)` };
  const giver = spots.filter((s) => s.item === id).sort(soonest)[0];
  if (!giver) return { item: id, name: readable(id) };
  return { item: id, name: itemOf(giver), giver };
}

/** The spot's rule with each item it names picked out, an item Link starts
 * with marked "(start)". Alternatives the rule writes as `A|B` read "A or
 * B". */
export function rule(spot: Spot, spots: Spot[], start: Held[] = []): Part[] {
  const need = spot.need ?? "";
  const ids = spot.need_items ?? [];
  if (!need) return [];
  if (ids.length === 0) return [{ text: need }];
  const longest = [...ids].sort((a, b) => b.length - a.length).map(literal);
  const pattern = new RegExp(`\\b(${longest.join("|")})\\b`);
  return need
    .split(pattern)
    .filter(Boolean)
    .map((piece) =>
      ids.includes(piece)
        ? item(piece, spots, start)
        : { text: piece.replaceAll("|", " or ") },
    );
}

/** A rule as a tree: a word, or every or any of its parts. */
type Expr = { word: string } | { op: "and" | "or"; of: Expr[] };

/** `need` as a tree, or null when it won't parse. `|` binds tightest, then
 * `and`, then `or`, and brackets group. */
function parse(need: string): Expr | null {
  const tokens = need.match(/[()|]|[^\s()|]+/g) ?? [];
  let i = 0;
  const chain =
    (op: "and" | "or", next: () => Expr, sep: string) => (): Expr => {
      const of = [next()];
      while (tokens[i] === sep) {
        i++;
        of.push(next());
      }
      return of.length === 1 ? (of[0] as Expr) : { op, of };
    };
  const word = (): Expr => {
    const t = tokens[i++];
    if (t === "(") {
      const e = any();
      if (tokens[i++] !== ")") throw new Error("unclosed bracket");
      return e;
    }
    if (!t || [")", "|", "and", "or"].includes(t)) throw new Error(t);
    return { word: t };
  };
  const any = chain("or", chain("and", chain("or", word, "|"), "and"), "or");
  try {
    const e = any();
    return i === tokens.length ? e : null;
  } catch {
    return null;
  }
}

/** Whether `e` passes with `has` saying which words hold. */
const passes = (e: Expr, has: (word: string) => boolean): boolean =>
  "word" in e
    ? has(e.word)
    : e.op === "and"
      ? e.of.every((p) => passes(p, has))
      : e.of.some((p) => passes(p, has));

/** Whether the rule can't pass without an item, or takes it as one option
 * of several. */
export type Need = "all" | "any";

/** Each item `spot`'s rule names, by the key of the card giving it: a
 * giving check's hash, or the id of an item Link starts with. Read from
 * {@link rule}, so the glow and the detail line never disagree. A spot never
 * needs itself, and an item nothing gives has no card. */
function givers(
  spot: Spot,
  spots: Spot[],
  start: Held[],
): [key: string, item: string][] {
  return rule(spot, spots, start).flatMap((p): [string, string][] => {
    if (!("item" in p)) return [];
    if (p.giver)
      return p.giver.hash === spot.hash ? [] : [[p.giver.hash, p.item]];
    return start.some((h) => h.item === p.item) ? [[p.item, p.item]] : [];
  });
}

/** What `spot`'s rule names, by card key, each `"all"` when the rule fails
 * without it and `"any"` when another option does. Words that aren't items
 * hold, and so does an item something gives. A rule that won't parse, or
 * can't pass anyway, needs all it names. */
export function needed(
  spot: Spot,
  spots: Spot[],
  start: Held[] = [],
): Map<string, Need> {
  const items = new Set(spot.need_items ?? []);
  const given = new Set([
    ...spots.map((s) => s.item),
    ...start.map((h) => h.item),
  ]);
  const expr = parse(spot.need ?? "");
  const holds = (without?: string) =>
    expr !== null &&
    passes(expr, (w) => !items.has(w) || (w !== without && given.has(w)));
  const optional = holds();
  return new Map(
    givers(spot, spots, start).map(([key, item]) => [
      key,
      optional && holds(item) ? "any" : "all",
    ]),
  );
}

/** The checks whose rules need `picked`, by hash. */
export function unlocks(picked: Spot, spots: Spot[], start: Held[]) {
  const opened = spots.filter(
    (s) =>
      s.hash !== picked.hash &&
      s.need_items?.includes(picked.item) &&
      givers(s, spots, start).some(([key]) => key === picked.hash),
  );
  return new Set(opened.map((s) => s.hash));
}

/** A rule's parts as one line of text. */
export const ruleText = (parts: Part[]) =>
  parts.map((p) => ("text" in p ? p.text : p.name)).join("");
