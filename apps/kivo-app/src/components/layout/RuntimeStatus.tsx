/** The sidebar's live status line: "Connected · Ready", "Reconnecting…" (ARCH-04). */
import { useRuntime } from "../../ipc/runtime";
import { cn } from "../../lib/cn";
import { viewLink } from "../../lib/session";

export function RuntimeStatus({ onOpen }: { onOpen: () => void }) {
  const { link } = useRuntime();
  const view = viewLink(link);
  return (
    <button type="button" className="k-side__status" onClick={onOpen} title={view.detail}>
      <span className={cn("k-side__dot", `is-${view.tone}`)} aria-hidden />
      <span role="status">{view.status}</span>
    </button>
  );
}
