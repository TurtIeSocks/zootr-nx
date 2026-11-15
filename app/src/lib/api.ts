type Result<T> = { status: "ok"; data: T } | { status: "error"; error: string };
/** A command's data, or its error message thrown. */
export async function call<T>(result: Promise<Result<T>>): Promise<T> {
  const r = await result;
  if (r.status === "error") throw new Error(r.error);
  return r.data;
}

/** Whether two values hold the same data. */
export const same = (a: unknown, b: unknown) =>
  JSON.stringify(a) === JSON.stringify(b);

/** An updater that keeps the old value when `next` holds the same data, so
 * nothing reading it redraws and forms holding it keep their edits. */
export const keep =
  <T>(next: T) =>
  (prev: T): T =>
    same(prev, next) ? prev : next;

/** What an error says, whatever was thrown. */
export function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}
