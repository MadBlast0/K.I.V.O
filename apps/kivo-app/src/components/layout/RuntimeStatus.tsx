import { useTranslation } from "react-i18next";
import { useRuntime } from "../../ipc/runtime";
import { cn } from "../../lib/cn";
import { viewLink } from "../../lib/session";

/** The sidebar's footer, as in the mockup: KIVO's state and its permission mode beside a small
 * orb; before it's connected, the connection's state. Opens Home. */
export function RuntimeStatus({ onOpen }: { onOpen: () => void }) {
  const { t } = useTranslation();
  const { link } = useRuntime();
  const view = viewLink(link);
  const snapshot = link?.status === "connected" ? link.snapshot : null;
  if (!snapshot) {
    return (
      <button type="button" className="k-side__status" onClick={onOpen} title={view.detail}>
        <span className={cn("k-side__dot", `is-${view.tone}`)} aria-hidden />
        <span role="status">{view.status}</span>
      </button>
    );
  }
  const live = snapshot.session === "listening" || snapshot.session === "speaking";
  const state = t(`session.${snapshot.session}.short`);
  const mode = t(`shell.modeLine.${snapshot.mode}`);
  return (
    <button
      type="button"
      className="k-side__status k-side__status--live"
      onClick={onOpen}
      title={view.detail}
      aria-label={`${state} · ${mode}`}
    >
      <span className={cn("k-side__orb", live && "is-live")} aria-hidden>
        <span />
      </span>
      <span className="k-side__state" role="status">
        <b>{state}</b>
        <span>{mode}</span>
      </span>
    </button>
  );
}
