import { Gem, Heart, type LucideIcon, Sparkle } from "lucide-react";
import type { DevSwitches } from "@/bindings";
import { Tip } from "@/components/tip";
import { send, setSwitches, useCanSend, useDev } from "@/stores/dev";

/** Each dev switch, its chip's label and icon, and what it does. */
const SWITCHES: {
  name: keyof DevSwitches;
  label: string;
  icon: LucideIcon;
  tip: string;
}[] = [
  { name: "health", label: "Health", icon: Heart, tip: "Keeps hearts full" },
  { name: "magic", label: "Magic", icon: Sparkle, tip: "Keeps magic full" },
  {
    name: "resources",
    label: "Resources",
    icon: Gem,
    tip: "Keeps every ammo and rupees full",
  },
];

/** A toggle chip for each infinite switch, showing what the game holds:
 * the last state read, or what a toggle read back since. */
export function InfinitySwitches() {
  return (
    <div className="grid grid-cols-3 gap-2">
      {SWITCHES.map((s) => (
        <SwitchChip key={s.name} {...s} />
      ))}
    </div>
  );
}

/** Toggles one switch, showing what the game holds for it. */
function SwitchChip({
  name,
  label,
  icon: Icon,
  tip,
}: (typeof SWITCHES)[number]) {
  const [on] = useDev.usePath(`game.state.switches.${name}`, false);
  const can = useCanSend();
  const what = `infinite ${label.toLowerCase()}`;

  return (
    <Tip tip={tip} wrap>
      <button
        type="button"
        aria-pressed={on}
        disabled={!can}
        onClick={() =>
          send("switch", `turn ${what} ${on ? "off" : "on"}`, async () => {
            const now = await setSwitches(name, !on);
            return `Infinite ${label.toLowerCase()} ${now[name] ? "on" : "off"}`;
          })
        }
        className="inline-flex h-7 w-full min-w-0 cursor-pointer items-center justify-center gap-1.5 rounded-md border border-line-strong bg-surface px-2 font-medium text-[12.5px] text-ink-muted transition-colors duration-(--duration-fast) enabled:hover:bg-surface-hover disabled:cursor-not-allowed disabled:opacity-45 aria-pressed:border-transparent aria-pressed:bg-primary-soft aria-pressed:text-primary-ink [&_svg]:size-3.5 [&_svg]:shrink-0"
      >
        <Icon aria-hidden />
        <span className="truncate">{label}</span>
      </button>
    </Tip>
  );
}
