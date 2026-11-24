import { History, RefreshCw, Sigma, Variable } from "lucide-react";
import { useEffect, useState } from "react";
import { commands, type DevValue, type Info } from "@/bindings";
import { NameField } from "@/components/name-field";
import { linkClass } from "@/components/styles";
import { Button } from "@/components/ui/button";
import { ValueInput } from "@/components/value-input";
import { DevCard } from "@/dev/dev-card";
import { openTab } from "@/dev/nav";
import { RECENT_VALUES } from "@/dev/requests";
import { useDevValue } from "@/dev/use-dev-value";
import { call } from "@/lib/api";
import { SAVED } from "@/lib/quest-step";
import { send, useCanSend, useDev } from "@/stores/dev";
import { remember, useUi } from "@/stores/ui";

/** Reads a GameData value by name and writes it back, shows how a formula
 * is worked out, and loads a name set lately. */
export function ValuesTab() {
  const value = useValue();
  return (
    <div className="flex flex-col gap-2.5">
      <GameDataCard value={value} />
      {value.value?.expr && <Formula expr={value.value.expr} />}
      <RecentValues />
    </div>
  );
}

type Shown = ReturnType<typeof useValue>;

/** The name field, then what the picked name is and holds, a field for
 * its type, and Set. Or why it couldn't be read. */
function GameDataCard({ value: shown }: { value: Shown }) {
  const { name, value, error, again } = shown;
  const writable = value && !value.expr && value.info.input !== "read";

  return (
    <DevCard title="GameData" icon={Variable}>
      <ValueSelect />
      {error && <p className="mt-2 text-danger-ink text-xs">{error}</p>}
      {value && (
        <dl className="mt-3 grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-[12.5px]">
          <dt className="text-ink-faint">Type</dt>
          <dd className="type-mono">{value.info.kind}</dd>
          {!value.expr && (
            <>
              <dt className="text-ink-faint">Default</dt>
              <dd className="type-mono break-all">
                {value.info.default ?? "—"}
              </dd>
              <dt className="text-ink-faint">Game Holds</dt>
              <dd className="type-mono break-all">{value.live ?? "—"}</dd>
            </>
          )}
        </dl>
      )}
      {value && !value.expr && (
        <>
          <div className="mt-3 flex items-start gap-1.5">
            <ValueField info={value.info} />
            <Button
              size="icon"
              variant="ghost"
              aria-label="Read again"
              onClick={again}
            >
              <RefreshCw />
            </Button>
            {writable && <SetValueButton again={again} />}
          </div>
          {origin(value) && (
            <p className="mt-1.5 text-ink-faint text-xs">{origin(value)}</p>
          )}
          {name && <SavedStep name={name} />}
        </>
      )}
    </DevCard>
  );
}

/** Loads `name` in the name field. */
const load = (name: string) => useUi.setPath("dev.set.name", name);

/** A BoolExp's formula. The game keeps no value for it, so each name in
 * it loads that name, to set instead. */
function Formula({ expr }: { expr: string }) {
  const [names] = useDev.usePath("options.names");

  // Each part keyed by where it starts in the formula.
  let at = 0;
  const parts = expr.split(/([\w.]+)/).map((text) => {
    const part = { text, at };
    at += text.length;
    return part;
  });
  return (
    <DevCard
      title="Worked Out, Not Stored"
      icon={Sigma}
      meta="set the values in it"
    >
      <p className="text-ink-muted text-xs">
        Worked out from the values in it, so set those instead
      </p>
      <p className="type-mono mt-2 break-words rounded-md bg-surface-sunken px-2.5 py-2 text-xs">
        {parts.map(({ text, at }) =>
          names.includes(text) ? (
            <button
              key={at}
              type="button"
              className={linkClass}
              onClick={() => load(text)}
            >
              {text}
            </button>
          ) : (
            <span key={at}>{text}</span>
          ),
        )}
      </p>
    </DevCard>
  );
}

/** The names set lately, with the value set, each loaded with a click. */
function RecentValues() {
  const [recent] = useUi.usePath("dev.recent.values");
  if (recent.length === 0) return null;

  return (
    <DevCard title="Recent" icon={History}>
      <ul aria-label="Recent values" className="-mx-3 -mb-1">
        {recent.map((r) => (
          <li key={r.name}>
            <button
              type="button"
              onClick={() => load(r.name)}
              className="flex w-full cursor-pointer items-center gap-2 px-3 py-1 text-left text-[13px] hover:bg-surface-hover"
            >
              <span className="type-mono truncate">{r.name}</span>
              <span className="type-mono ml-auto shrink-0 text-ink-faint">
                {r.value}
              </span>
            </button>
          </li>
        ))}
      </ul>
    </DevCard>
  );
}

/** For a quest's saved step: a note that writing it leaves the quest
 * where it was, and a way to move the quest itself in Give. */
function SavedStep({ name }: { name: string }) {
  const [quests] = useDev.usePath("options.quests");
  const [value] = useDev.usePath("gameValue");
  const challenge = name.startsWith(SAVED) ? name.slice(SAVED.length) : "";
  if (!quests[challenge]) return null;

  return (
    <p className="mt-1.5 text-ink-faint text-xs">
      The save's copy of {challenge}: setting it leaves the quest where it was.{" "}
      <button
        type="button"
        className={linkClass}
        onClick={() => {
          useUi.setPath("dev.quest", { challenge, step: value });
          openTab("give");
        }}
      >
        Move the Quest
      </button>
    </p>
  );
}

/** Writes the field to the picked name, then reads it back either way. A
 * write that works heads Recent. */
function SetValueButton({ again }: { again: () => void }) {
  const can = useCanSend();

  return (
    <Button
      disabled={!can}
      onClick={() => {
        const name = useUi.getPath("dev.set.name").trim();
        const value = useDev.getPath("gameValue");
        void send("set", `set ${name}`, async () => {
          try {
            const said = await call(commands.devSet(name, value));
            useUi.setPath("dev.recent.values", (r) =>
              remember(
                r,
                { name, value },
                RECENT_VALUES,
                (a, b) => a.name === b.name,
              ),
            );
            return said;
          } finally {
            again();
          }
        });
      }}
    >
      Set
    </Button>
  );
}

/** The GameData name to read, suggested from the game's names. */
function ValueSelect() {
  const [names] = useDev.usePath("options.names");
  const [name, setName] = useUi.usePath("dev.set.name");

  return (
    <NameField
      label="GameData Name"
      names={names}
      value={name}
      onChange={setName}
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

  const [round, setRound] = useState(0);
  const shown = useDevValue(name, round);

  useEffect(() => {
    if (shown?.value)
      setValue(shown.value.live ?? shown.value.info.default ?? "");
  }, [shown, setValue]);

  // Nothing of the last name stays up while this one loads.
  return {
    ...shown,
    name,
    again: () => setRound((r) => r + 1),
  };
}

/** The field for a value's type, holding the picked value. */
function ValueField({ info }: { info: Info }) {
  const [value, setValue] = useDev.usePath("gameValue");
  return <ValueInput info={info} value={value} onChange={setValue} />;
}

/** Where the shown value came from, when it isn't the game's. */
function origin({ game, live }: DevValue) {
  if (live !== null) return undefined;
  return game
    ? "The game doesn't hold this value, showing the default"
    : "The game isn't running, showing the default";
}
