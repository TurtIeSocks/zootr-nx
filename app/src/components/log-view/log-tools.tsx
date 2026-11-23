import { Check, Copy, FolderOpen, Search } from "lucide-react";
import { commands } from "@/bindings";
import { TipButton } from "@/components/tip-button";
import { Input } from "@/components/ui/input";
import { call } from "@/lib/api";
import { MOD } from "@/lib/platform";
import { useRequest } from "@/lib/request";

/** Props for {@link GrepInput}. */
interface GrepInputProps {
  value: string;
  onChange: (grep: string) => void;
}

/** Filters a log's lines by what they say. */
export function GrepInput({ value, onChange }: GrepInputProps) {
  return (
    <div className="relative w-44">
      <Search className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-ink-faint" />
      <Input
        aria-label="Filter lines"
        placeholder="Filter lines"
        className="h-7 pl-8"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      />
    </div>
  );
}

/** Props for {@link CopyButton}. */
interface CopyButtonProps {
  /** How many lines it copies: the selected ones, or every one shown. */
  count: number;
  selecting: boolean;
  copied: boolean;
  onCopy: () => void;
}

/** Copies the selected lines, or every line shown, one per line. */
export function CopyButton({
  count,
  selecting,
  copied,
  onCopy,
}: CopyButtonProps) {
  const label = selecting
    ? `Copy ${count} ${count === 1 ? "row" : "rows"}`
    : "Copy shown";
  const what = selecting ? "the selected rows" : "every row shown";
  return (
    <TipButton
      label={label}
      tip={copied ? "Copied" : `Copy ${what}, one line each (${MOD}C)`}
      variant="ghost"
      disabled={count === 0}
      onClick={onCopy}
    >
      {copied ? <Check /> : <Copy />}
    </TipButton>
  );
}

/** Props for {@link OpenLogButton}. */
interface OpenLogButtonProps {
  /** The log file, if there is one. */
  log?: string | null;
  /** What the button shows, as "Show the … in its folder". */
  what: string;
}

/** Shows a log file in its folder, through the page's request. */
export function OpenLogButton({ log, what }: OpenLogButtonProps) {
  const { run } = useRequest();
  return (
    <TipButton
      label="Open log"
      tip={`Show the ${what} in its folder`}
      variant="ghost"
      disabled={!log}
      onClick={() => log && run(() => call(commands.revealLog(log)))}
    >
      <FolderOpen />
    </TipButton>
  );
}
