import {
  Check,
  ChevronRight,
  FlaskConical,
  type LucideIcon,
  Pin,
  RotateCcw,
  Share2,
} from "lucide-react";
import { type ReactNode, useEffect, useState } from "react";
import { commands, type SharedBuild } from "@/bindings";
import { Callout, InlineError } from "@/components/callout";
import { ChangeList } from "@/components/change-list";
import { Help } from "@/components/help";
import { linkClass } from "@/components/styles";
import { ChangedDot } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { call, message } from "@/lib/api";
import {
  type FullTest,
  fullTest,
  type Settings,
  testOn,
  testParts,
} from "@/lib/config";
import { count, isShareCode, nameOf } from "@/lib/format";
import { savedAs } from "@/lib/plando";
import { type MarkedChip, markedChips } from "@/lib/settings";
import { cn } from "@/lib/utils";
import { groupIcon, type Section } from "@/shell/nav";
import { useConfig } from "@/stores/config";
import {
  DEFAULTS_FROM,
  takeSettings,
  useDriftBase,
  useDriftFrom,
  useSeeds,
} from "@/stores/seeds";
import { openSettings } from "@/stores/shell";
import { useUi } from "@/stores/ui";

/** A tile shows this many chips, then "+N more". */
const SHOWN = 6;

/** What the next seed builds with: a tile per settings group, then the
 * extras. With a share code in the seed field, the code's own. */
export function BuildOverview() {
  const [seed] = useUi.usePath("newSeed.seed");
  return isShareCode(seed) ? <CodeOverview code={seed} /> : <OwnOverview />;
}

/** The current settings and extras, marked against what drift measures
 * from. */
function OwnOverview() {
  const [settings] = useConfig.usePath("config.settings");
  const [test] = useConfig.usePath("config.test");
  const pins = useConfig((s) => Object.keys(s.config.place).length);
  const name = useConfig((s) => savedAs(s.config));
  const base = useDriftBase();
  return (
    <div className="flex flex-col gap-2.5">
      <div className="flex items-center gap-2">
        <div className="mr-auto flex items-center gap-1.5">
          <p className="type-overline">Builds with</p>
          <Help topic="builds-with" />
        </div>
        <DriftPill />
      </div>
      <Tiles
        settings={settings}
        base={base?.settings ?? {}}
        plando={plandoView(pins, name)}
        test={testView(test)}
      />
    </div>
  );
}

/** Props for {@link CodeOverview}. */
interface CodeOverviewProps {
  code: string;
}

/** A share code's settings and extras, which leave the person's alone. */
function CodeOverview({ code }: CodeOverviewProps) {
  const read = useShared(code);
  return (
    <div className="flex flex-col gap-2.5">
      <Callout
        tone="info"
        icon={Share2}
        action={
          <button
            type="button"
            className={linkClass}
            onClick={() => useUi.setPath("newSeed.seed", "")}
          >
            Clear code
          </button>
        }
      >
        <b className="font-semibold">Building from the share code.</b> It brings
        its own settings and extras; yours stay as they are
      </Callout>
      {"error" in read ? (
        <InlineError error={read.error} />
      ) : (
        read.build && (
          <>
            <p className="type-overline">The code builds with</p>
            <Tiles
              settings={read.build.settings}
              base={null}
              plando={plandoView(read.build.pins, null)}
              test={testView(fullTest(read.build.test))}
              off="Not in this code"
            />
          </>
        )
      )}
    </div>
  );
}

/** A share code read into what it builds with, or why it couldn't be. */
type Read = { build?: SharedBuild } | { error: string };

/** Reads `code` whenever it changes. */
function useShared(code: string): Read {
  const [read, setRead] = useState<Read>({});
  useEffect(() => {
    let live = true;
    setRead({});
    call(commands.shareSettings(code)).then(
      (build) => live && setRead({ build }),
      (e: unknown) => live && setRead({ error: message(e) }),
    );
    return () => {
      live = false;
    };
  }, [code]);
  return read;
}

/** An extra that's on: its tile's count and chips. */
interface ExtraView {
  meta: string;
  chips: string[];
}

/** The plando's tile: its saved name and how many checks it pins. Null
 * when it pins none. */
const plandoView = (pins: number, name: string | null): ExtraView | null =>
  pins === 0
    ? null
    : {
        meta: `${pins} pinned`,
        chips: [...(name ? [name] : []), `${count(pins, "check")} pinned`],
      };

/** The test setup's tile: where it starts and what it adds. Null while
 * it changes nothing. */
const testView = (test: FullTest): ExtraView | null =>
  testOn(test)
    ? {
        meta: "On",
        chips: testParts(test, (give) => count(give.length, "item")),
      }
    : null;

/** Props for {@link Tiles}. */
interface TilesProps {
  settings: Settings;
  /** What the chips are marked against, or null for no marks. */
  base: Settings | null;
  plando: ExtraView | null;
  test: ExtraView | null;
  /** What an extra that's off says under its name. */
  off?: string;
}

/** A tile per settings group, in schema order, then the extras. */
function Tiles({ settings, base, plando, test, off }: TilesProps) {
  const [schema] = useConfig.usePath("schema");
  const [groups] = useConfig.usePath("groups");
  const extras: [string, Section, LucideIcon, ExtraView | null][] = [
    ["Plando", "plando", Pin, plando],
    ["Test setup", "test", FlaskConical, test],
  ];
  return (
    <div className="grid grid-cols-3 gap-2">
      {groups.map(({ name }) => {
        const defs = schema.filter((d) => d.group === name);
        const chips = markedChips(defs, schema, settings, base);
        const on = chips.filter((c) => c.mark !== "gone").length;
        return (
          <Tile
            key={name}
            icon={groupIcon(name)}
            name={name}
            meta={on > 0 ? `${on} on` : undefined}
            section={name}
          >
            {chips.length > 0 ? (
              <Chips chips={chips} />
            ) : (
              <Empty>Defaults</Empty>
            )}
          </Tile>
        );
      })}
      <p className="type-overline col-span-full mt-2">Extras</p>
      {extras.map(([name, section, icon, view]) => (
        <Tile
          key={section}
          icon={icon}
          name={name}
          meta={view?.meta ?? "Off"}
          section={section}
          off={!view}
        >
          {view ? (
            <Chips chips={view.chips.map((label) => ({ id: label, label }))} />
          ) : (
            off && <Empty>{off}</Empty>
          )}
        </Tile>
      ))}
    </div>
  );
}

/** Props for {@link Tile}. */
interface TileProps {
  icon: LucideIcon;
  name: string;
  /** Quiet text at the head's trailing edge: a count, "On" or "Off". */
  meta?: string;
  /** The Settings page a click opens. */
  section: Section;
  /** An extra that's off: its name reads quieter. */
  off?: boolean;
  children: ReactNode;
}

/** A group or extra at a glance. A click opens its page. */
function Tile({ icon: Icon, name, meta, section, off, children }: TileProps) {
  return (
    <button
      type="button"
      onClick={() => openSettings(section)}
      className="group flex min-h-[92px] min-w-0 cursor-pointer flex-col gap-2 rounded-md border border-line bg-surface p-3 text-left outline-none transition-colors duration-(--duration-fast) ease-out hover:border-line-strong hover:bg-surface-hover focus-visible:border-focus focus-visible:shadow-[0_0_0_1px_var(--focus)]"
    >
      <span
        className={cn(
          "flex w-full items-center gap-2 font-semibold text-[13px] text-ink",
          off && "font-medium text-ink-muted",
        )}
      >
        <Icon className="size-[15px] shrink-0 text-ink-muted" />
        {name}
        {meta && (
          <span className="ml-auto font-[450] text-[12px] text-ink-faint tabular-nums">
            {meta}
          </span>
        )}
        <ChevronRight
          aria-hidden
          className={cn(
            "-ml-1 size-3.5 shrink-0 text-ink-faint opacity-0 transition-opacity duration-(--duration-fast) group-hover:opacity-100 group-focus-visible:opacity-100",
            !meta && "ml-auto",
          )}
        />
      </span>
      {children}
    </button>
  );
}

/** Quiet words in a tile with no chips. */
function Empty({ children }: { children: ReactNode }) {
  return <span className="text-[12.5px] text-ink-faint">{children}</span>;
}

/** Props for {@link Chips}. */
interface ChipsProps {
  chips: MarkedChip[];
}

/** A tile's first chips, then how many more. */
function Chips({ chips }: ChipsProps) {
  const more = chips.length - SHOWN;
  return (
    <span className="flex flex-wrap gap-1">
      {chips.slice(0, SHOWN).map((c) => (
        <ChipView key={c.id} chip={c} />
      ))}
      {more > 0 && (
        <span className="inline-flex h-[22px] items-center px-1 text-[12px] text-ink-faint">
          +{more} more
        </span>
      )}
    </span>
  );
}

/** How each mark reads, for those who can't see its colour. */
const MARK_WORDS = { new: "new", gone: "gone" } as const;

/** A chip: gold with a dot when new, outlined and struck through when
 * gone. A changed choice says on hover what it was. */
function ChipView({ chip }: { chip: MarkedChip }) {
  const { label, value, mark, was } = chip;
  const body = (
    <span
      data-mark={mark}
      className={cn(
        "inline-flex min-h-[22px] max-w-full items-center gap-[5px] rounded-sm bg-surface-sunken px-2 py-[5px] text-[12px] text-ink leading-[14px]",
        mark === "new" && "bg-gold-soft text-gold-ink",
        mark === "gone" &&
          "bg-transparent text-ink-faint line-through decoration-ink-faint shadow-[inset_0_0_0_1px_var(--line-strong)]",
      )}
    >
      {mark === "new" && <ChangedDot className="size-[5px] shrink-0" />}
      <span>
        {label}
        {value !== undefined && ":"}
      </span>
      {value !== undefined && (
        <b
          className={cn(
            "font-medium",
            mark === "new" ? "text-gold-ink" : "text-ink-muted",
          )}
        >
          {value}
        </b>
      )}
      {mark && <span className="sr-only"> ({MARK_WORDS[mark]})</span>}
    </span>
  );
  if (was === undefined) return body;
  return (
    <Tooltip>
      <TooltipTrigger asChild>{body}</TooltipTrigger>
      <TooltipContent>Was {was}</TooltipContent>
    </Tooltip>
  );
}

/** "since" a seed, "from" the defaults. */
const since = (from: string) => (from === DEFAULTS_FROM ? "from" : "since");

/** How many settings differ from what drift measures from, with Review, or
 * that nothing does. */
function DriftPill() {
  const [drift] = useSeeds.usePath("drift");
  const from = useDriftFrom();
  const pill =
    "inline-flex h-6 items-center gap-1.5 whitespace-nowrap rounded-full font-medium text-[12px] [&>svg]:size-[13px] [&>svg]:stroke-2";
  if (drift.length === 0) {
    return (
      <span className={cn(pill, "bg-primary-soft px-2 text-primary-ink")}>
        <Check />
        Same as {from}
      </span>
    );
  }
  return (
    <span className={cn(pill, "bg-gold-soft pr-1 pl-2 text-gold-ink")}>
      <ChangedDot />
      {count(drift.length, "change")} {since(from)} {from}
      <Review />
    </span>
  );
}

/** Every difference by group, and a way back to what drift measures
 * from. */
function Review() {
  const [open, setOpen] = useState(false);
  const [drift] = useSeeds.usePath("drift");
  const base = useDriftBase();
  const from = useDriftFrom();
  const restore =
    from === DEFAULTS_FROM
      ? "Use the default settings"
      : `Use ${from}'s settings`;
  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="h-5 cursor-pointer rounded-full px-2 text-gold-ink hover:bg-gold-ink/10 aria-expanded:bg-gold-ink/10"
        >
          Review
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-[360px] p-3">
        <p className="type-overline pb-1">
          Changed {since(from)} {(base && nameOf(base)) || from}
        </p>
        <ChangeList changes={drift} />
        <div className="-mx-3 my-2.5 h-px bg-line" />
        <div className="flex justify-end">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => {
              setOpen(false);
              takeSettings(base ?? { settings: {} });
            }}
          >
            <RotateCcw />
            {restore}
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}
