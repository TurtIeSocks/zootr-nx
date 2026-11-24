import { Tip } from "@/components/tip";
import { useUpdate } from "@/stores/update";

/** Which build this is, so a screenshot always says: a release's version,
 * or the commit a source build came from, with its version in the tip. */
export function Version() {
  const [current] = useUpdate.usePath("current");
  const [commit] = useUpdate.usePath("commit");
  const [release] = useUpdate.usePath("release");
  const version = current && `v${current}`;
  const source = !release && commit;
  return (
    <Tip tip={source && version} focusable>
      <span className="mr-auto pl-2 font-mono text-ink-faint text-xs">
        {source || version}
      </span>
    </Tip>
  );
}
