/** The plain word for each raw flag type, by its name without `Array`.
 * Types missing here read as their raw name. */
const WORDS: Record<string, string> = {
  Bool: "On/Off",
  Int: "Number",
  UInt: "Number",
  Int64: "Number",
  UInt64: "Number",
  Float: "Decimal",
  Enum: "Choice",
  Vector3: "Position",
  String16: "Text",
  String32: "Text",
  String64: "Text",
  WString16: "Text",
  Binary: "Data",
};

/** The suffix that makes a raw type a list of its element type. */
const ARRAY = "Array";

/** Whether a raw type is a list. */
export const isArray = (kind: string) => kind.endsWith(ARRAY);

/** A raw type's element type: itself, or a list's without `Array`. */
export const elementKind = (kind: string) =>
  isArray(kind) ? kind.slice(0, -ARRAY.length) : kind;

/** What the Flags tab calls a raw type: its plain word, "<word> List" for
 * an array, or the raw name itself with developer tools on. */
export function kindWord(kind: string, dev: boolean): string {
  if (dev) return kind;
  const base = elementKind(kind);
  const word = WORDS[base] ?? base;
  return isArray(kind) ? `${word} List` : word;
}

/** The type filter's options: each word once, with the raw types it
 * covers, in the order the types first show. */
export function kindOptions(
  kinds: string[],
  dev: boolean,
): { word: string; kinds: string[] }[] {
  const options: { word: string; kinds: string[] }[] = [];
  for (const kind of kinds) {
    const word = kindWord(kind, dev);
    const option = options.find((o) => o.word === word);
    if (option) option.kinds.push(kind);
    else options.push({ word, kinds: [kind] });
  }
  return options;
}
