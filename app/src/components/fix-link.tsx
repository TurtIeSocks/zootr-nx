import { fix, fixable } from "@/stores/shell";

/** A reason nothing can be built or installed, as a link to where it's
 * fixed when a page fixes it, else as plain text. */
export function FixLink({ why }: { why: string }) {
  if (!fixable(why)) return why;
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
