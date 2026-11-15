import { useState } from "react";
import { Input } from "@/components/ui/input";
import { matching } from "@/lib/matching";
import { cn } from "@/lib/utils";

/** Props for {@link NameField}. */
export interface NameFieldProps {
  /** Names the field, and is its placeholder unless one is given. */
  label: string;
  /** What it suggests. Empty, it takes any text without suggesting. */
  names: string[];
  value: string;
  onChange: (v: string) => void;
  /** For a {@link Field}'s label to point at. */
  id?: string;
  placeholder?: string;
  /** Enter with no suggestion open, as when adding what's typed. */
  onEnter?: () => void;
  /** A quiet note after a suggestion, such as why it can't be used yet. */
  note?: (name: string) => string | undefined;
}

/** A text field suggesting the names that match what's typed. The list is
 * drawn in the page, since the webview puts a datalist's popup far from its
 * field. */
export function NameField({
  label,
  names,
  value,
  onChange,
  id,
  placeholder,
  onEnter,
  note,
}: NameFieldProps) {
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);

  const shown = open ? matching(names, value) : [];

  const pick = (name: string) => {
    onChange(name);
    setOpen(false);
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (!shown.length) {
      if (e.key === "Enter" && onEnter) {
        e.preventDefault();
        onEnter();
      }
      return;
    }
    const picked = shown[active];
    if (e.key === "ArrowDown")
      setActive(Math.min(active + 1, shown.length - 1));
    else if (e.key === "ArrowUp") setActive(Math.max(active - 1, 0));
    else if (e.key === "Enter" && picked) pick(picked);
    else if (e.key === "Escape") setOpen(false);
    else return;
    e.preventDefault();
  };

  return (
    <div className="relative min-w-0 flex-1">
      <Input
        id={id}
        aria-label={id ? undefined : label}
        placeholder={placeholder ?? label}
        autoComplete="off"
        value={value}
        onChange={(e) => {
          onChange(e.target.value);
          setOpen(true);
          setActive(0);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onKeyDown={onKeyDown}
      />
      {shown.length > 0 && (
        <ul
          aria-label={`${label} suggestions`}
          className="absolute inset-x-0 top-full z-10 mt-1 max-h-48 overflow-auto rounded-md border border-line bg-surface-raised py-1 shadow-md"
        >
          {shown.map((n, i) => {
            const quiet = note?.(n);
            return (
              <li key={n}>
                <button
                  type="button"
                  tabIndex={-1}
                  onMouseDown={(e) => e.preventDefault()}
                  onClick={() => pick(n)}
                  className={cn(
                    "flex w-full cursor-pointer gap-2 px-2 py-0.5 text-left text-[13px] hover:bg-surface-hover",
                    i === active && "bg-surface-hover",
                    quiet && "text-ink-muted",
                  )}
                >
                  <span className="truncate">{n}</span>
                  {quiet && (
                    <span className="ml-auto shrink-0 text-ink-faint text-xs">
                      {quiet}
                    </span>
                  )}
                </button>
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
}
