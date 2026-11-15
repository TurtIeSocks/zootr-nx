import { commands } from "@/bindings";
import { NameField } from "@/components/name-field";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { DevSection } from "@/dev/section";
import { call } from "@/lib/api";
import { useRequest } from "@/lib/request";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

/** Gives Link an item, as many as the count says. */
export function Give() {
  return (
    <DevSection title="Give">
      <div className="flex gap-1.5">
        <GiveSelect />
        <GiveCount />
        <GiveButton />
      </div>
    </DevSection>
  );
}

/** How many to give. */
function GiveCount() {
  const [count, setCount] = useUi.usePath("dev.give.count");

  return (
    <Input
      aria-label="Count"
      type="number"
      className="w-16"
      value={count}
      onChange={(e) => setCount(Number(e.target.value))}
    />
  );
}

/** The item to give, suggested from the game's item names. */
function GiveSelect() {
  const [items] = useDev.usePath("options.items");
  const [item, setItem] = useUi.usePath("dev.give.item");

  return (
    <NameField label="Item" names={items} value={item} onChange={setItem} />
  );
}

/** Sends the give, once an item is named and the game can be read. */
function GiveButton() {
  const [ready] = useDev.usePath("ready");
  const [item] = useUi.usePath("dev.give.item");
  const [count] = useUi.usePath("dev.give.count");
  const request = useRequest();

  return (
    <Button
      variant="secondary"
      disabled={!ready || request.busy || !item.trim()}
      onClick={() =>
        request.run(() => call(commands.devGive(item.trim(), count)))
      }
    >
      Give
    </Button>
  );
}
