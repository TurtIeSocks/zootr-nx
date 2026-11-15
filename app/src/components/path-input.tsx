import { open } from "@tauri-apps/plugin-dialog";
import { FolderOpen } from "lucide-react";
import type { ComponentProps } from "react";
import { InlineError } from "@/components/callout";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useRequestState } from "@/lib/request";
import { cn } from "@/lib/utils";

/** Props for {@link PathInput}. */
export interface PathInputProps
  extends Omit<ComponentProps<typeof Input>, "value" | "onChange"> {
  value: string;
  onChange: (value: string) => void;
  /** A macOS .app is a file to the picker. */
  pick: "folder" | "file";
  /** Extensions the file picker offers, without dots. */
  extensions?: string[];
}

/** A path field with a Browse button that fills it from the system picker,
 * saying why under it if the picker fails. */
export function PathInput({
  value,
  onChange,
  pick,
  extensions,
  className,
  ...props
}: PathInputProps) {
  const browse = useRequestState();
  const pickPath = () =>
    browse.run(async () => {
      const path = await open({
        directory: pick === "folder",
        defaultPath: value || undefined,
        filters: extensions && [{ name: extensions.join(", "), extensions }],
      });
      if (path) onChange(path);
    });
  return (
    <div className="flex flex-1 flex-col gap-1">
      <div className="flex gap-2">
        <Input
          {...props}
          className={cn("type-mono", className)}
          value={value}
          onChange={(e) => onChange(e.target.value)}
        />
        <Button variant="secondary" disabled={browse.busy} onClick={pickPath}>
          <FolderOpen /> Browse
        </Button>
      </div>
      <InlineError error={browse.error} />
    </div>
  );
}
