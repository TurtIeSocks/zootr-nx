import { DUMP_GONE, NEED_DUMP, NEED_TARGET } from "@/lib/config";
import { fix } from "@/stores/shell";

const FIXABLE = [NEED_DUMP, DUMP_GONE, NEED_TARGET];

/** A reason nothing can be built or installed, as a link to where it's
 * fixed when a page fixes it, else as plain text. */
export function FixLink({ why }: { why: string }) {
  if (!FIXABLE.includes(why)) return why;
  return (
    <button
      type="button"
      className="cursor-pointer font-medium underline underline-offset-2"
      onClick={() => fix(why)}
    >
      {why}
    </button>
  );
}
