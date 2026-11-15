import { useEffect, useState } from "react";
import { commands, type DevValue, type Info } from "@/bindings";
import { NameField } from "@/components/name-field";
import { Button } from "@/components/ui/button";
import { ValueInput } from "@/components/value-input";
import { DevSection } from "@/dev/section";
import { call, message } from "@/lib/api";
import { useRequest } from "@/lib/request";
import { useDev } from "@/stores/dev";
import { useUi } from "@/stores/ui";

/** Reads a GameData value by name, and writes it back. */
export function SetValue() {
  return (
    <DevSection title="Set">
      <ValueSelect />
      <DisplayValue />
    </DevSection>
  );
}

/** Wraps a setter so an edit also drops what the last Set said, since it
 * goes stale once either field changes. */
function useEdit() {
  const { clear } = useRequest();
  return (set: (v: string) => void) => (v: string) => {
    clear();
    set(v);
  };
}

/** The picked value's type, a field holding it, and where it came from, or
 * why it couldn't be read. */
function DisplayValue() {
  const { value: shown, error, again } = useValue();
  const writable = shown && shown.info.input !== "read";

  return (
    <>
      {error && <p className="mt-1.5 text-danger-ink text-xs">{error}</p>}
      {!!shown && (
        <>
          <p className="mt-1.5 text-ink-muted text-xs">{shown.info.kind}</p>
          <div className="mt-1.5 flex gap-1.5">
            <ValueField info={shown.info} />
            {writable && <SetValueButton again={again} />}
          </div>
          {origin(shown) && (
            <p className="mt-1 text-ink-faint text-xs">{origin(shown)}</p>
          )}
        </>
      )}
    </>
  );
}

/** Writes the field to the picked name, then reads it back either way. */
function SetValueButton({
  again,
}: {
  again: ReturnType<typeof useValue>["again"];
}) {
  const [ready] = useDev.usePath("ready");
  const request = useRequest();

  return (
    <Button
      variant="secondary"
      disabled={!ready || request.busy}
      onClick={() =>
        request.run(async () => {
          try {
            const name = useUi.getPath("dev.set.name");
            const value = useDev.getPath("gameValue");

            return await call(commands.devSet(name.trim(), value));
          } finally {
            again();
          }
        })
      }
    >
      Set
    </Button>
  );
}

/** The GameData name to read, suggested from the game's names. */
function ValueSelect() {
  const [names] = useDev.usePath("options.names");
  const [name, setName] = useUi.usePath("dev.set.name");
  const edit = useEdit();

  return (
    <NameField
      label="GameData name"
      names={names}
      value={name}
      onChange={edit(setName)}
    />
  );
}

/** A name the game could hold: one the list has, or a `0x` hash. */
function isName(names: string[], name: string) {
  return names.includes(name) || /^0x[0-9a-f]+$/i.test(name);
}

/** What the picked name is and holds, reread when the name changes or
 * `again` is called. Each read fills the field with what it holds. */
function useValue() {
  const [unsafeName] = useUi.usePath("dev.set.name");
  const [names] = useDev.usePath("options.names");
  const [, setValue] = useDev.usePath("gameValue");

  const name = isName(names, unsafeName.trim()) ? unsafeName.trim() : "";

  const [shown, setShown] = useState<{
    name: string;
    value?: DevValue;
    error?: string;
  }>();
  const [round, setRound] = useState(0);

  useEffect(() => {
    // Read again after `round` moves on.
    void round;
    if (!name) return;
    let live = true;
    call(commands.devValue(name))
      .then((value) => live && setShown({ name, value }))
      .catch((e) => live && setShown({ name, error: message(e) }));
    return () => {
      live = false;
    };
  }, [name, round]);

  useEffect(() => {
    if (shown?.value)
      setValue(shown.value.live ?? shown.value.info.default ?? "");
  }, [shown, setValue]);

  // Nothing of the last name stays up while this one loads.
  return {
    ...(shown?.name === name ? shown : {}),
    again: () => setRound((r) => r + 1),
  };
}

/** The field for a value's type, holding the picked value. */
function ValueField({ info }: { info: Info }) {
  const [value, setValue] = useDev.usePath("gameValue");
  const edit = useEdit();
  return <ValueInput info={info} value={value} onChange={edit(setValue)} />;
}

/** Where the shown value came from, when it isn't the game's. */
function origin({ game, live }: DevValue) {
  if (live !== null) return undefined;
  return game
    ? "The game doesn't hold this value, showing the default"
    : "The game isn't running, showing the default";
}
