import { X } from "lucide-react";
import * as React from "react";
import { fieldClass } from "@/components/styles";
import { cn } from "@/lib/utils";

/** Props for {@link Input}. */
export interface InputProps extends React.ComponentProps<"input"> {
  /** Shows a × that empties the field while it holds text. On by default
   * for text and search fields, never for others. */
  clearable?: boolean;
  /** Classes for the box around a clearable field: its layout in the row
   * (`flex-1`, a width), where `className` styles the field itself. */
  boxClassName?: string;
}

/** Types a × makes sense on. */
const TEXT = new Set([undefined, "text", "search"]);

/** Empties `input` as typing would, so its `onChange` and every store
 * behind it see the change, then puts the cursor back in it. */
function clear(input: HTMLInputElement) {
  const set = Object.getOwnPropertyDescriptor(
    HTMLInputElement.prototype,
    "value",
  )?.set;
  set?.call(input, "");
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.focus();
}

function Input({
  className,
  type,
  clearable = true,
  boxClassName,
  ref,
  onChange,
  ...props
}: InputProps) {
  const own = React.useRef<HTMLInputElement | null>(null);
  const [typed, setTyped] = React.useState(Boolean(props.defaultValue));
  const wraps = clearable && TEXT.has(type);
  const field = (
    <input
      type={type}
      data-slot="input"
      ref={(el) => {
        own.current = el;
        if (typeof ref === "function") ref(el);
        else if (ref) ref.current = el;
      }}
      className={cn(fieldClass, "h-8 px-3", wraps && "pr-7", className)}
      onChange={(e) => {
        setTyped(e.target.value !== "");
        onChange?.(e);
      }}
      {...props}
    />
  );
  if (!wraps) return field;

  const filled = props.value === undefined ? typed : String(props.value) !== "";
  return (
    <div className={cn("relative w-full min-w-0", boxClassName)}>
      {field}
      {filled && !props.disabled && !props.readOnly && (
        <button
          type="button"
          aria-label="Clear"
          tabIndex={-1}
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => own.current && clear(own.current)}
          className="absolute top-1/2 right-1.5 flex size-5 -translate-y-1/2 cursor-pointer items-center justify-center rounded-sm text-ink-faint hover:bg-surface-hover hover:text-ink [&_svg]:size-3.5"
        >
          <X />
        </button>
      )}
    </div>
  );
}

export { Input };
