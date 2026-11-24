import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";

/** Props for {@link HiddenMenu}. */
interface HiddenMenuProps<T extends string> {
  /** The button's text, before how many are hidden. */
  label: string;
  /** The menu's heading. */
  heading: string;
  options: T[];
  hidden: T[];
  onHiddenChange: (hidden: T[]) => void;
  /** How many of the options are hidden, when not every hidden one is
   * offered. */
  off?: number;
  disabled?: boolean;
  className?: string;
  itemClassName?: string;
}

/** A menu of checkboxes picking which options a log shows. */
export function HiddenMenu<T extends string>({
  label,
  heading,
  options,
  hidden,
  onHiddenChange,
  off = hidden.length,
  disabled,
  className,
  itemClassName,
}: HiddenMenuProps<T>) {
  const show = (x: T, on: boolean) =>
    onHiddenChange(on ? hidden.filter((h) => h !== x) : [...hidden, x]);
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button size="sm" variant="ghost" disabled={disabled}>
          {label}
          {off > 0 && <span className="text-ink-faint">· {off} hidden</span>}
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className={className}>
        <DropdownMenuLabel>{heading}</DropdownMenuLabel>
        {options.map((x) => (
          <DropdownMenuCheckboxItem
            key={x}
            checked={!hidden.includes(x)}
            onCheckedChange={(on) => show(x, on)}
            onSelect={(e) => e.preventDefault()}
            className={itemClassName}
          >
            {x}
          </DropdownMenuCheckboxItem>
        ))}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
