import { CircleAlert } from "lucide-react";
import { Callout, InlineError } from "@/components/callout";
import { FixLink } from "@/components/fix-link";
import { useNoDump } from "@/stores/seeds";

/** For a page that reads the game: with no dump, a link to choose one. */
export function NeedsDump() {
  const why = useNoDump();
  if (!why) return null;
  return (
    <Callout tone="gold" icon={CircleAlert}>
      <FixLink why={why} />
    </Callout>
  );
}

/** A read of the game's `error`, or with no dump, the link to choose one
 * in its place: the error only says the dump is missing. */
export function DumpError({ error }: { error?: string }) {
  const why = useNoDump();
  if (why) return <NeedsDump />;
  return <InlineError error={error} />;
}
