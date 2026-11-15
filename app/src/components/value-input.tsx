import type { Info } from "@/bindings";
import { fieldClass } from "@/components/styles";
import { Input } from "@/components/ui/input";
import { Textarea } from "@/components/ui/textarea";
import { cn } from "@/lib/utils";

/** Props for {@link ValueInput}. */
export interface ValueInputProps {
  /** The value's type, from `dev_value`. */
  info: Info;
  value: string;
  onChange: (v: string) => void;
  /** Its accessible name. */
  label?: string;
}

/** The field for a GameData value's type: a select of its choices, a
 * number, or text. Types that can't be written show it read-only. */
export function ValueInput({
  info,
  value,
  onChange,
  label = "Value",
}: ValueInputProps) {
  const named = { "aria-label": label };
  const change = (e: { target: { value: string } }) => onChange(e.target.value);

  if (info.input === "choice") {
    // A value the definition doesn't list still shows as picked.
    const options = !value || info.choices.includes(value) ? [] : [value];
    return (
      <select
        {...named}
        className={cn(fieldClass, "h-8 px-2")}
        value={value}
        onChange={change}
      >
        {[...options, ...info.choices].map((c) => (
          <option key={c} value={c}>
            {c}
          </option>
        ))}
      </select>
    );
  }
  if (info.input === "read") {
    return (
      <Textarea
        {...named}
        readOnly
        rows={Math.min(value.split("\n").length, 8)}
        className="type-mono"
        value={value || "nothing to show"}
      />
    );
  }
  return (
    <Input
      {...named}
      type={info.input === "number" ? "number" : "text"}
      step={info.kind === "Float" ? "any" : 1}
      value={value}
      onChange={change}
    />
  );
}
