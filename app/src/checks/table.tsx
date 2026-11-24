import { Meter } from "@/components/meter";
import { cn } from "@/lib/utils";

/** Props for {@link AreaHeading}. */
interface AreaHeadingProps {
  name: string;
  done: number;
  total: number;
  /** The heading's borders. */
  className?: string;
}

/** A group's sticky heading row, with its count. It pins at `top-8`,
 * just below the columns' names. */
export function AreaHeading({
  name,
  done,
  total,
  className,
}: AreaHeadingProps) {
  return (
    <tr>
      <th
        colSpan={5}
        className={cn(
          "sticky top-8 z-1 bg-surface px-3 pt-3 pb-2 text-left font-normal",
          className,
        )}
      >
        <div className="flex items-center gap-3">
          <span className="font-semibold text-[13.5px]">{name}</span>
          <Meter value={done} max={total}>
            {done}/{total}
          </Meter>
        </div>
      </th>
    </tr>
  );
}
