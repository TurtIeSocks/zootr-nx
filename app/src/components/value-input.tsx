import type { Info } from "@/bindings";
import { fieldClass } from "@/components/styles";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
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
  /** Sizing from the row it sits in. Fills the row when unset. */
  className?: string;
  /** What each choice reads as, where it isn't the value itself. */
  labels?: Record<string, string>;
}

/** The field for a GameData value's type: a switch for a Bool, a select
 * of its choices, a number, a number per axis for a vector, or text.
 * Types that can't be written show it read-only. */
export function ValueInput({
  info,
  value,
  onChange,
  label = "Value",
  className,
  labels,
}: ValueInputProps) {
  const named = { "aria-label": label };
  const change = (e: { target: { value: string } }) => onChange(e.target.value);

  if (info.kind === "Bool") {
    return (
      // The row's sizing goes on a box around it, so the switch keeps its own.
      <div className={cn("flex h-8 items-center", className)}>
        <Switch
          {...named}
          checked={value === "true"}
          onCheckedChange={(on) => onChange(on ? "true" : "false")}
        />
      </div>
    );
  }
  if (info.input === "vector") {
    return (
      <VectorInput
        kind={info.kind}
        value={value}
        onChange={onChange}
        label={label}
        className={className}
      />
    );
  }
  if (info.input === "choice") {
    // A value the definition doesn't list still shows as picked.
    const options = !value || info.choices.includes(value) ? [] : [value];
    return (
      <select
        {...named}
        className={cn(fieldClass, "h-8 px-2", className)}
        value={value}
        onChange={change}
      >
        {[...options, ...info.choices].map((c) => (
          <option key={c} value={c}>
            {labels?.[c] ?? c}
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
        className={cn("type-mono", className)}
        value={value || "nothing to show"}
      />
    );
  }
  return (
    <Input
      {...named}
      className={className}
      type={info.input === "number" ? "number" : "text"}
      step={info.kind === "Float" ? "any" : 1}
      value={value}
      onChange={change}
    />
  );
}

/** Props for {@link VectorInput}. */
interface VectorInputProps {
  /** `Vector2` or `Vector3`. */
  kind: string;
  /** Comma-separated, as the save holds it. */
  value: string;
  onChange: (v: string) => void;
  label: string;
  className?: string;
}

/** The axes this many numbers name. */
const AXES = ["x", "y", "z"];

/** One number field per axis, joined back with commas. */
function VectorInput({
  kind,
  value,
  onChange,
  label,
  className,
}: VectorInputProps) {
  const n = kind.startsWith("Vector2") ? 2 : 3;
  const parts = value.split(",");
  const set = (i: number, v: string) =>
    onChange(
      AXES.slice(0, n)
        .map((_, j) => (j === i ? v : (parts[j] ?? "0").trim()))
        .join(","),
    );
  return (
    <div className={cn("flex gap-1", className)}>
      {AXES.slice(0, n).map((axis, i) => (
        <Input
          key={axis}
          aria-label={`${label} ${axis}`}
          type="number"
          step="any"
          value={(parts[i] ?? "").trim()}
          onChange={(e) => set(i, e.target.value)}
        />
      ))}
    </div>
  );
}
