import type { Level } from "@/bindings";
import { InlineError } from "@/components/callout";
import { Help } from "@/components/help";
import { HiddenMenu } from "@/components/log-view/hidden-menu";
import { type LineProps, PICKED } from "@/components/log-view/line";
import {
  CopyButton,
  GrepInput,
  OpenLogButton,
} from "@/components/log-view/log-tools";
import { Tail } from "@/components/log-view/tail";
import { useLogCopy } from "@/components/log-view/use-log-copy";
import { useSelection } from "@/components/log-view/use-selection";
import { CutText, Tip } from "@/components/tip";
import { Badge } from "@/components/ui/badge";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { cn } from "@/lib/utils";
import { type AppLog, LEVELS, textOf, visible } from "@/logs/lines";
import { Page } from "@/shell/page-header";
import { useAppLog } from "@/stores/app-log";
import { useDevTools } from "@/stores/config";
import { useUi } from "@/stores/ui";

/** Each level's badge. */
const BADGES: Record<Level, "danger" | "gold" | "default"> = {
  error: "danger",
  warn: "gold",
  info: "default",
  debug: "default",
};

/** The lines shown, after the filter and the hidden levels. */
function useShown() {
  const [lines] = useAppLog.usePath("lines");
  const [grep] = useUi.usePath("logs.grep");
  const [hidden] = useUi.usePath("logs.hidden");
  return visible(lines, grep, hidden, useDevTools());
}

/** Filters the log by what its lines say. */
function LogsGrep() {
  const [grep, setGrep] = useUi.usePath("logs.grep");
  return <GrepInput value={grep} onChange={setGrep} />;
}

/** Picks which levels the log shows. Debug is there only with developer
 * tools on. */
function LevelsMenu() {
  const [hidden, setHidden] = useUi.usePath("logs.hidden");
  const levels = useDevTools() ? LEVELS : LEVELS.filter((l) => l !== "debug");
  const off = levels.filter((l) => hidden.includes(l)).length;
  return (
    <HiddenMenu
      label="Levels"
      heading="Show These Levels"
      options={levels}
      hidden={hidden}
      onHiddenChange={setHidden}
      off={off}
      itemClassName="capitalize"
    />
  );
}

/** Says where the log is written, or why it isn't. */
function FileNote() {
  const [unsaved] = useAppLog.usePath("unsaved");
  return (
    <div className="flex min-w-0 items-center gap-2">
      <span className="font-medium text-sm">App Log</span>
      <Help topic="app-logs" />
      {unsaved && (
        <Tip focusable tip={unsaved}>
          <Badge variant="danger">Not Saving to a File</Badge>
        </Tip>
      )}
    </div>
  );
}

/** Shows this run's log file in its folder. */
function LogsFileButton() {
  const [file] = useAppLog.usePath("file");
  return <OpenLogButton log={file} what="app's log" />;
}

/** Why the page's request or the store last failed. */
function LogsError() {
  const { error } = useRequest();
  const [failed] = useAppLog.usePath("error");
  return <InlineError error={error ?? failed} className="px-4 pt-2" />;
}

/** Props for {@link LogRow}. */
interface LogRowProps extends LineProps {
  line: AppLog["lines"][number];
}

/** One line: when, how bad, and what. A cut-off line shows whole on
 * hover. */
function LogRow({ line, selected, pointer }: LogRowProps) {
  return (
    <div
      data-line
      data-selected={selected || undefined}
      {...pointer}
      className={cn(
        PICKED,
        "grid grid-cols-[auto_3.5rem_minmax(0,1fr)] items-center gap-3 px-4 py-0.75 transition-colors duration-(--duration-fast) hover:bg-surface-hover",
      )}
    >
      <span className="text-ink-faint tabular-nums">{line.time}</span>
      <Badge shape="square" variant={BADGES[line.level]}>
        {line.level}
      </Badge>
      <CutText text={line.text} />
    </div>
  );
}

/** The app's own log, live. */
export function LogsView() {
  const shown = useShown();
  const selection = useSelection();
  const copyButton = useLogCopy(shown, selection, textOf);

  return (
    <Page wide className="min-h-0 flex-1">
      <RequestProvider>
        <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-surface shadow-xs">
          <div className="flex flex-wrap items-center gap-x-3 gap-y-2 border-line border-b px-3 py-2">
            <FileNote />
            <div className="ml-auto flex items-center gap-2">
              <LogsGrep />
              <LevelsMenu />
              <CopyButton {...copyButton} />
              <LogsFileButton />
            </div>
          </div>
          <LogsError />
          <Tail
            shown={shown}
            next={() => useAppLog.getState().next}
            selection={selection}
          >
            {shown.map((l) => (
              <LogRow key={l.id} line={l} {...selection.line(l.id, shown)} />
            ))}
            {shown.length === 0 && (
              <p className="px-4 py-12 text-center font-sans text-ink-muted text-sm">
                Nothing logged yet
              </p>
            )}
          </Tail>
        </section>
      </RequestProvider>
    </Page>
  );
}
