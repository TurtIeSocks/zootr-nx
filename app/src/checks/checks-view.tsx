import { Circle, CircleCheck, Ellipsis, Search } from "lucide-react";
import { type FormEvent, useState } from "react";
import type { Row } from "@/bindings";
import { EntranceRows } from "@/checks/entrance-rows";
import {
  type Area,
  areas,
  type Edit,
  FILTERS,
  isDone,
  nameOf,
} from "@/checks/rows";
import { InlineError } from "@/components/callout";
import { Help } from "@/components/help";
import { LoadGate } from "@/components/load-gate";
import { Meter } from "@/components/meter";
import { RequestError } from "@/components/request-error";
import { SaveFooter } from "@/components/save-footer";
import { Tip } from "@/components/tip";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Input } from "@/components/ui/input";
import { Segmented } from "@/components/ui/segmented";
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip";
import { useRequest } from "@/lib/request";
import { RequestProvider } from "@/lib/request-provider";
import { cn } from "@/lib/utils";
import { Page } from "@/shell/page-header";
import { mark, useChecks } from "@/stores/checks";
import { useDevTools } from "@/stores/config";
import { label } from "@/stores/live";
import { closeCheckText, openCheckText, useShell } from "@/stores/shell";
import { useUi } from "@/stores/ui";

/** Props for {@link TextDialog}. */
interface TextDialogProps {
  edit: Edit;
  row: Row;
}

/** The check text dialog, while a check's note or name is edited and its
 * row is still there. */
function EditingText() {
  const [editing] = useShell.usePath("checkText");
  const row = useChecks((s) =>
    s.rows.find((r) => r.spot.hash === editing?.hash),
  );
  if (!editing || !row) return null;
  // Another check or field starts a fresh form.
  return (
    <TextDialog
      key={`${editing.edit} ${editing.hash}`}
      edit={editing.edit}
      row={row}
    />
  );
}

/** One line of text on a check. Saving it blank clears it. */
function TextDialog({ edit, row }: TextDialogProps) {
  return (
    <Dialog open onOpenChange={(open) => !open && closeCheckText()}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>
            {edit === "Name" ? "Rename" : "Note on"} {nameOf(row)}
          </DialogTitle>
        </DialogHeader>
        <RequestProvider>
          <TextForm edit={edit} row={row} />
        </RequestProvider>
      </DialogContent>
    </Dialog>
  );
}

/** Saves a name as the flag's label, as the Live tab does, or a note as
 * the row's mark. */
function saveText(edit: Edit, row: Row, text: string) {
  if (edit === "Note")
    return mark(row.spot.hash, text ? { kind: "Note", text } : null);
  return row.flag === null ? Promise.resolve() : label(row.flag, text);
}

/** The text dialog's form, saving through the request around it. */
function TextForm({ edit, row }: TextDialogProps) {
  const [text, setText] = useState(
    edit === "Name"
      ? (row.label ?? "")
      : row.mark?.kind === "Note"
        ? row.mark.text
        : "",
  );
  const save = useRequest();

  const submit = (e: FormEvent) => {
    e.preventDefault();
    save.run(async () => {
      await saveText(edit, row, text.trim());
      closeCheckText();
    });
  };

  return (
    <form className="flex flex-col gap-4" onSubmit={submit}>
      <Input
        autoFocus
        aria-label={edit}
        placeholder={edit}
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      <SaveFooter />
    </form>
  );
}

/** Props for the components that show one check. */
interface RowProps {
  row: Row;
}

/** Skip, note, rename or clear a check's mark, through the row's request.
 * Note and Rename open the text dialog. */
function MarkMenu({ row }: RowProps) {
  const marking = useRequest();
  const edit = (e: Edit) => openCheckText(row.spot.hash, e);
  const setMark = (m: Parameters<typeof mark>[1]) =>
    marking.run(() => mark(row.spot.hash, m));
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          size="icon-sm"
          variant="ghost"
          aria-label={`Mark ${nameOf(row)}`}
        >
          <Ellipsis />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem onSelect={() => setMark({ kind: "Skipped" })}>
          Skip
        </DropdownMenuItem>
        <DropdownMenuItem onSelect={() => edit("Note")}>Note…</DropdownMenuItem>
        {row.flag !== null && (
          <DropdownMenuItem onSelect={() => edit("Name")}>
            Rename…
          </DropdownMenuItem>
        )}
        {row.mark && (
          <DropdownMenuItem onSelect={() => setMark(null)}>
            Clear mark
          </DropdownMenuItem>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

/** "hidden" until the game reports it, or "untracked" when it never will. */
function Unseen({ row }: RowProps) {
  const unseen = "text-ink-faint italic";
  if (row.flag !== null) return <span className={unseen}>hidden</span>;
  return (
    <Tooltip>
      <TooltipTrigger asChild tabIndex={0}>
        <span className={cn(unseen, "cursor-default")}>untracked</span>
      </TooltipTrigger>
      <TooltipContent>Its flag isn't known, so it never ticks</TooltipContent>
    </Tooltip>
  );
}

/** What every cell in a check's row shares. */
const cell = "h-8.5 border-line border-t px-3 align-middle";

/** A check's row, with its own request for marking it: one mark at a
 * time, and why one failed shown in the row. */
function CheckRow({ row }: RowProps) {
  const devTools = useDevTools();
  const done = isDone(row);
  return (
    <RequestProvider>
      <tr
        className={cn(
          "transition-colors duration-(--duration-fast) hover:bg-surface-hover",
          done && "text-ink-faint",
        )}
      >
        <td className={cn(cell, "w-10 pr-0")}>
          {row.checked ? (
            <CircleCheck className="size-4 text-primary" aria-label="checked" />
          ) : (
            <Circle className="size-4 text-control-border" aria-hidden />
          )}
        </td>
        <Tip tip={devTools && `${row.spot.holder} ${row.spot.hash}`}>
          <td
            className={cn(cell, done && "line-through decoration-line-strong")}
          >
            {nameOf(row)}
          </td>
        </Tip>
        <td className={cell}>
          {row.checked ? (
            row.spot.item_name || row.spot.item
          ) : (
            <Unseen row={row} />
          )}
        </td>
        <td className={cn(cell, "text-ink-muted")}>
          {row.mark?.kind === "Skipped" ? (
            <Badge shape="square">Skipped</Badge>
          ) : (
            row.mark?.text
          )}
          <RequestError />
        </td>
        <td className={cn(cell, "w-12 text-right")}>
          <MarkMenu row={row} />
        </td>
      </tr>
    </RequestProvider>
  );
}

/** Props for {@link AreaRows}. */
interface AreaRowsProps {
  area: Area;
}

/** An area's heading, with its count, over its checks. */
function AreaRows({ area }: AreaRowsProps) {
  return (
    <tbody className="group/area">
      <tr>
        <th
          colSpan={5}
          className="sticky top-0 z-1 border-line-strong border-t bg-surface px-3 pt-3 pb-2 text-left font-normal group-first/area:border-t-0"
        >
          <div className="flex items-center gap-3">
            <span className="font-semibold text-[13.5px]">{area.name}</span>
            <Meter value={area.done} max={area.total}>
              {area.done}/{area.total}
            </Meter>
          </div>
        </th>
      </tr>
      {area.rows.map((r) => (
        <CheckRow key={r.spot.hash} row={r} />
      ))}
    </tbody>
  );
}

/** Picks which rows show. */
function FilterPicker() {
  const [filter, setFilter] = useUi.usePath("checks.filter");
  return (
    <Segmented options={FILTERS} value={filter} onValueChange={setFilter} />
  );
}

/** How many checks are done, of all of them. */
function Done() {
  const [done] = useChecks.usePath("done");
  const [total] = useChecks.usePath("total");
  return (
    <Meter value={done} max={total} className="text-[13px]">
      <span>
        <b className="font-semibold text-ink">{done}</b>/{total} done
      </span>
    </Meter>
  );
}

/** The search over areas and check names. */
function Query() {
  const [query, setQuery] = useUi.usePath("checks.query");
  return (
    <div className="relative ml-auto min-w-40 max-w-64 flex-1">
      <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-ink-faint" />
      <Input
        className="h-7 pl-8"
        placeholder="Filter checks"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
    </div>
  );
}

/** The shuffled doors, then the shown checks by area. */
function Areas() {
  const [rows] = useChecks.usePath("rows");
  const [query] = useUi.usePath("checks.query");
  const [filter] = useUi.usePath("checks.filter");
  return (
    <table className="w-full border-separate border-spacing-0 text-[13px]">
      <EntranceRows />
      {areas(rows, query, filter).map((a) => (
        <AreaRows key={a.name} area={a} />
      ))}
    </table>
  );
}

/** Every check in the installed seed, ticked as Link opens them. */
export function ChecksView() {
  const [loaded] = useChecks.usePath("loaded");
  const [error] = useChecks.usePath("error");
  return (
    <LoadGate loaded={loaded} error={error}>
      <Checks />
    </LoadGate>
  );
}

/** The loaded checks, or a line saying no seed is installed. */
function Checks() {
  const installed = useChecks((s) => s.id !== null);
  const [error] = useChecks.usePath("error");
  if (!installed)
    return (
      <p className="py-12 text-center text-ink-muted">No seed is installed</p>
    );
  return (
    <Page wide className="min-h-0 flex-1">
      <section className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-lg border border-line bg-surface shadow-xs">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2 border-line border-b px-3 py-2">
          <FilterPicker />
          <Done />
          <Query />
          <span className="ml-auto flex">
            <Help topic="checks" />
          </span>
        </div>
        <InlineError error={error} className="px-3 pt-2" />
        <div className="min-h-0 flex-1 overflow-auto">
          <Areas />
        </div>
      </section>
      <EditingText />
    </Page>
  );
}
