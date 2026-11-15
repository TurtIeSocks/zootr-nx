import type { DevSwitches } from "@/bindings";
import { Switch } from "@/components/ui/switch";
import { DevSection } from "@/dev/section";
import { useRequest } from "@/lib/request";
import { setSwitches, useDev } from "@/stores/dev";

/** Each dev switch and its label. */
const SWITCHES: { key: keyof DevSwitches; label: string }[] = [
  { key: "health", label: "Infinite health" },
  { key: "magic", label: "Infinite magic" },
  { key: "resources", label: "Infinite resources" },
];

/** A toggle for each dev switch, showing what the game holds: the last
 * state read, or what a toggle read back since. */
export function InfinitySwitches() {
  return (
    <DevSection title="Switches">
      <div className="flex flex-col gap-1.5">
        {SWITCHES.map(({ key, label }) => (
          <div
            key={key}
            className="flex items-center justify-between gap-2 text-[13px]"
          >
            {label}
            <InfinitySwitch name={key} label={label} />
          </div>
        ))}
      </div>
    </DevSection>
  );
}

/** Toggles one switch, showing what the game holds for it. */
function InfinitySwitch({
  name,
  label,
}: {
  name: keyof DevSwitches;
  label: string;
}) {
  const [value] = useDev.usePath(`game.state.switches.${name}`, false);
  const [ready] = useDev.usePath("ready");
  const request = useRequest();

  return (
    <Switch
      aria-label={label}
      checked={value}
      disabled={!ready || request.busy}
      onCheckedChange={(on) =>
        request.run(async () => {
          const now = await setSwitches(name, on);
          return `${label.toLowerCase()} ${now[name] ? "on" : "off"}`;
        })
      }
    />
  );
}
