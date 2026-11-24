import { X } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, type Info } from "@/bindings";
import { Field } from "@/components/field";
import { NameField } from "@/components/name-field";
import { QuestStepPicker } from "@/components/quest-step-picker";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Segmented } from "@/components/ui/segmented";
import { ValueInput } from "@/components/value-input";
import { call } from "@/lib/api";
import { known, listed } from "@/lib/game-options";
import { type QuestStep, stepOf, stepsOf } from "@/lib/quest-step";
import { SIDES } from "@/lib/sides";
import { useLoad } from "@/lib/use-load";
import { type TestForm, useTestForm } from "@/settings/test-form";

/** Props for {@link TextInput}. */
interface TextInputProps {
  /** The form field it edits. */
  name: "expect";
  id: string;
  placeholder: string;
}

/** A one-line input bound to a form field. */
export function TextInput({ name, ...props }: TextInputProps) {
  const { form, set } = useTestForm();
  return (
    <Input
      {...props}
      value={form[name]}
      onChange={(e) => set({ [name]: e.target.value })}
    />
  );
}

/** Where a new game starts, warning when the dump never sends Link there. */
export function SpawnField() {
  const { form, set, options } = useTestForm();
  const spawn = form.spawn.trim();
  return (
    <Field
      id="test-spawn"
      label="Spawn"
      hint="MAP/SPOT, adult on an _Adult map. Blank starts in Link's house"
      warn={
        spawn && !known(options, "spawns", spawn)
          ? "The game never sends Link there. Pick one from the list"
          : undefined
      }
    >
      <NameField
        id="test-spawn"
        label="Spawn"
        placeholder="Link's house"
        names={listed(options, "spawns")}
        value={form.spawn}
        onChange={(spawn) => set({ spawn })}
      />
    </Field>
  );
}

/** A map's placed actors, each once, or none for no map. */
const actorsOf = async (map: string): Promise<string[]> =>
  map
    ? [...new Set((await call(commands.devActors(map))).map((a) => a.actor))]
    : [];

/** A placed actor of the spawn's map to warp beside, and which side. */
export function NearField() {
  const { form, set } = useTestForm();
  const map = form.spawn.trim().split("/")[0] ?? "";
  const actors = useLoad(actorsOf, map);
  return (
    <Field
      id="test-near"
      label="Near"
      hint="A placed actor of the spawn's map. Apply warps Link to that side of it once there"
    >
      <div className="flex gap-2">
        <NameField
          id="test-near"
          label="Near"
          placeholder="an actor's name"
          names={actors.status === "ok" ? actors.data : []}
          value={form.near}
          onChange={(near) => set({ near })}
        />
        <Segmented
          aria-label="Side"
          options={SIDES}
          value={form.side}
          onValueChange={(side) => set({ side })}
        />
      </div>
    </Field>
  );
}

/** Props for {@link Chips}. */
interface ChipsProps {
  items: string[];
  onRemove: (item: string) => void;
}

/** Each item as a chip with a × to drop it. */
function Chips({ items, onRemove }: ChipsProps) {
  if (!items.length) return null;
  return (
    <div className="flex flex-wrap gap-1.5">
      {items.map((item) => (
        <Badge key={item} className="type-mono pr-1">
          {item}
          <button
            type="button"
            aria-label={`Remove ${item}`}
            className="grid size-4 cursor-pointer place-items-center rounded-full hover:bg-surface-hover hover:text-ink"
            onClick={() => onRemove(item)}
          >
            <X className="size-3" />
          </button>
        </Badge>
      ))}
    </div>
  );
}

/** The pouch items to hand over: type one to add it, click its × to drop
 * it. */
export function ItemsField() {
  const { form, set, options } = useTestForm();
  /** The item being typed, before it's added. */
  const [item, setItem] = useState("");
  const typed = item.trim();
  const ok = !!typed && known(options, "items", typed);
  const add = () => {
    if (!ok || form.give.includes(typed)) return;
    set({ give: [...form.give, typed] });
    setItem("");
  };
  return (
    <Field
      id="test-item"
      label="Items"
      hint="Pouch items by actor, handed over before the first autosave"
      warn={typed && !ok ? "Not a pouch item" : undefined}
    >
      <div className="flex gap-2">
        <NameField
          id="test-item"
          label="Items"
          placeholder={
            options.status === "loading"
              ? "Reading the game…"
              : "an item's name"
          }
          names={listed(options, "items")}
          value={item}
          onChange={setItem}
          onEnter={add}
        />
        <Button
          variant="secondary"
          aria-label="Add item"
          onClick={add}
          disabled={!ok}
        >
          Add
        </Button>
      </div>
      <Chips
        items={form.give}
        onRemove={(g) => set({ give: form.give.filter((x) => x !== g) })}
      />
    </Field>
  );
}

/** The value beside a row's name, whichever field it is, so the name
 * keeps the rest of the row. */
const VALUE_WIDTH = "w-40 shrink-0";

/** A GameData value's type, or none for a blank name. */
const infoOf = async (name: string): Promise<Info | undefined> =>
  name ? (await call(commands.devValue(name))).info : undefined;

/** Props for {@link RowsField}. */
interface RowsFieldProps {
  /** The form field it edits, one `NAME=VALUE` a row. */
  name: Rows;
  label: string;
  /** What one row is, for its Add button. */
  noun: string;
  placeholder: string;
  hint: string;
}

/** Rows of `NAME=VALUE`: pick a name from the dump and a value its type
 * allows, then add it. A whole `NAME=VALUE` typed as the name adds as is. */
export function RowsField({
  name,
  label,
  noun,
  placeholder,
  hint,
}: RowsFieldProps) {
  const { options } = useTestForm();
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");
  const names = listed(options, "names");
  const typed = key.trim();
  const whole = typed.includes("=");
  const isKnown = names.includes(typed);
  const info = useLoad(infoOf, isKnown ? typed : "");
  const shown = info.status === "ok" ? info.data : undefined;

  // A newly read value starts at what a new game holds.
  useEffect(
    () => setValue(shown ? (shown.default ?? shown.choices[0] ?? "") : ""),
    [shown],
  );

  const line = whole ? typed : `${typed}=${value.trim()}`;
  const ok = whole || (!!typed && !!value.trim());
  const add = useAddRow(name, line, ok, () => setKey(""));
  const id = `test-${name}`;
  return (
    <Field
      id={id}
      label={label}
      hint={hint}
      warn={
        typed && !whole && names.length > 0 && !isKnown
          ? "The game has no value by that name"
          : undefined
      }
    >
      <div className="flex gap-2">
        <NameField
          id={id}
          label={label}
          placeholder={placeholder}
          names={names}
          value={key}
          onChange={setKey}
          onEnter={add}
        />
        {!whole &&
          (shown && shown.input !== "read" ? (
            <ValueInput
              info={shown}
              value={value}
              onChange={setValue}
              label={`${label} value`}
              className={VALUE_WIDTH}
            />
          ) : (
            <Input
              aria-label={`${label} value`}
              className={VALUE_WIDTH}
              placeholder="Value"
              value={value}
              onChange={(e) => setValue(e.target.value)}
              onKeyDown={(e) => e.key === "Enter" && add()}
            />
          ))}
        <Button
          variant="secondary"
          aria-label={`Add ${noun}`}
          onClick={add}
          disabled={!ok}
        >
          Add
        </Button>
      </div>
      <RowChips name={name} />
    </Field>
  );
}

/** Quest steps, run in order: pick a quest from the game's and one of its
 * steps, then add it. A whole `QUEST=STEP` typed as the name adds as is. */
export function QuestStepsField() {
  const { options } = useTestForm();
  const [pick, setPick] = useState<QuestStep>({ challenge: "", step: "" });
  const quests = options.status === "ok" ? options.data.quests : {};
  const typed = pick.challenge.trim();
  const whole = typed.includes("=");
  const step = stepOf(quests, pick);
  const line = whole ? typed : `${typed}=${step}`;
  const ok = whole || !!step;
  const add = useAddRow("progress", line, ok, () =>
    setPick({ challenge: "", step: "" }),
  );
  const label = "Quest Steps";
  const hasQuests = Object.keys(quests).length > 0;
  return (
    <Field
      id="test-progress"
      label={label}
      hint="Run in order. A new game runs them in its opening and Apply runs them live. The names are checked when you save"
      warn={
        typed && !whole && hasQuests && !stepsOf(quests, typed)
          ? "The game has no quest by that name"
          : undefined
      }
    >
      <QuestStepPicker
        id="test-progress"
        label={label}
        stepLabel={`${label} value`}
        placeholder="SageOf"
        quests={quests}
        value={pick}
        onChange={setPick}
        onEnter={add}
      >
        <Button
          variant="secondary"
          aria-label="Add quest step"
          onClick={add}
          disabled={!ok}
        >
          Add
        </Button>
      </QuestStepPicker>
      <RowChips name="progress" />
    </Field>
  );
}

/** The form fields made of `NAME=VALUE` rows. */
type Rows = "defaults" | "progress" | "goals";

/** Adds `line` to the rows of `name` once it's `ok` and not there yet,
 * then runs `done`. */
function useAddRow(name: Rows, line: string, ok: boolean, done: () => void) {
  const { form, set } = useTestForm();
  return () => {
    if (!ok || form[name].includes(line)) return;
    set({ [name]: [...form[name], line] } as Partial<TestForm>);
    done();
  };
}

/** The rows of `name`, each removable. */
function RowChips({ name }: { name: Rows }) {
  const { form, set } = useTestForm();
  return (
    <Chips
      items={form[name]}
      onRemove={(gone) =>
        set({
          [name]: form[name].filter((l) => l !== gone),
        } as Partial<TestForm>)
      }
    />
  );
}
