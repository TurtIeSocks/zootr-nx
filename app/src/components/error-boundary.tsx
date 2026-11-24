import { TriangleAlert } from "lucide-react";
import { Component, type ErrorInfo, type ReactNode } from "react";
import { Button } from "@/components/ui/button";
import { useCopied } from "@/lib/use-copied";
import { cn } from "@/lib/utils";

/** Props for {@link ErrorBoundary}. */
interface ErrorBoundaryProps {
  /** What it guards, as the notice names it: "Checks", "This window". */
  label: string;
  /** Clears a problem when it changes, so another view draws fresh. */
  resetKey?: unknown;
  className?: string;
  children: ReactNode;
}

interface ErrorBoundaryState {
  error?: Error;
  /** Where in the tree it broke, for the copied details. */
  where?: string;
}

/** Keeps a view that throws while drawing from blanking the window: it
 * shows what broke, with a way to try again and to copy the details. */
export class ErrorBoundary extends Component<
  ErrorBoundaryProps,
  ErrorBoundaryState
> {
  state: ErrorBoundaryState = {};

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    this.setState({ where: info.componentStack ?? undefined });
    console.error(`${this.props.label} hit a problem`, error);
  }

  componentDidUpdate(before: ErrorBoundaryProps) {
    if (this.state.error && before.resetKey !== this.props.resetKey)
      this.reset();
  }

  /** Clears the problem, so the view draws again. */
  reset = () => this.setState({ error: undefined, where: undefined });

  render() {
    const { error, where } = this.state;
    if (!error) return this.props.children;
    return (
      <CrashNotice
        label={this.props.label}
        error={error}
        where={where}
        className={this.props.className}
        onRetry={this.reset}
      />
    );
  }
}

/** Props for {@link CrashNotice}. */
interface CrashNoticeProps {
  label: string;
  error: Error;
  where?: string;
  className?: string;
  onRetry: () => void;
}

/** What broke, and what to do about it. */
function CrashNotice({
  label,
  error,
  where,
  className,
  onRetry,
}: CrashNoticeProps) {
  const details = [`${label}: ${error}`, error.stack, where]
    .filter(Boolean)
    .join("\n\n");
  const { copied, flash } = useCopied();
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(details);
      flash("details");
    } catch {
      // No clipboard to write to.
    }
  };
  return (
    <div
      role="alert"
      className={cn(
        "m-6 flex flex-col gap-3 rounded-lg border border-line bg-danger-soft p-4 text-[13px] text-ink",
        className,
      )}
    >
      <p className="flex items-center gap-2 font-semibold">
        <TriangleAlert className="size-4 text-danger-ink" />
        {label} hit a problem
      </p>
      <p className="text-ink-muted">
        The rest of zootr-nx still works. Try again, or copy the details for a
        bug report.
      </p>
      <pre className="max-h-32 overflow-auto whitespace-pre-wrap rounded-md bg-surface px-3 py-2 font-mono text-[12px]">
        {String(error)}
      </pre>
      <div className="flex gap-2">
        <Button size="sm" onClick={onRetry}>
          Try Again
        </Button>
        <Button size="sm" variant="secondary" onClick={() => void copy()}>
          {copied ? "Copied" : "Copy Details"}
        </Button>
      </div>
    </div>
  );
}
