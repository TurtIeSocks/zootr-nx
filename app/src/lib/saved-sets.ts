import { same } from "@/lib/api";
import type { FullConfig } from "@/lib/config";

/** A change to the config, worked out from the config it lands on. */
export type Change = (c: FullConfig) => Partial<FullConfig>;

/** Each draft the config keeps, and where the drafts kept by name beside
 * it go: the plando's pins, the Colors pane's picks. */
export interface SetKinds {
  place: "plandos";
  colors: "color_sets";
}

export const SET_KINDS: SetKinds = { place: "plandos", colors: "color_sets" };

/** A draft: a key to what it's set to. */
export type Draft = Record<string, unknown>;

/** Whether two drafts hold the same entries, whatever order their keys
 * came in. */
export const sameDraft = (a: Draft, b: Draft) => {
  const keys = Object.keys(a);
  return (
    keys.length === Object.keys(b).length && keys.every((k) => same(a[k], b[k]))
  );
};

/** Each entry of `rec` but `key`. */
export const without = <T>(
  rec: Record<string, T>,
  key: string,
): Record<string, T> =>
  Object.fromEntries(Object.entries(rec).filter(([k]) => k !== key));

/** The config's draft `draft` and its saved sets: which one it matches,
 * and the changes that load, keep and drop them. */
export function savedSets(draft: keyof SetKinds) {
  const sets = SET_KINDS[draft];
  const draftOf = (c: FullConfig) => c[draft] as Draft;
  const setsOf = (c: FullConfig) => c[sets] as Record<string, Draft>;
  /** The saved set the draft matches, if any. An empty draft matches
   * none. */
  const savedAs = (c: FullConfig) =>
    Object.keys(draftOf(c)).length === 0
      ? null
      : (Object.keys(setsOf(c))
          .sort()
          .find((n) => sameDraft(setsOf(c)[n] ?? {}, draftOf(c))) ?? null);
  return {
    draftOf,
    setsOf,
    savedAs,
    /** Whether the draft holds what no saved set keeps, so replacing it
     * loses it. */
    unsaved: (c: FullConfig) =>
      Object.keys(draftOf(c)).length > 0 && savedAs(c) === null,
    /** The saved set `name` as the draft. */
    load:
      (name: string): Change =>
      (c) => ({ [draft]: { ...setsOf(c)[name] } }),
    /** The draft kept as `name`, replacing a saved set by that name. */
    saveAs:
      (name: string): Change =>
      (c) => ({ [sets]: { ...setsOf(c), [name]: { ...draftOf(c) } } }),
    /** Drops the saved set `name`. The draft stays. */
    remove:
      (name: string): Change =>
      (c) => ({ [sets]: without(setsOf(c), name) }),
  };
}
