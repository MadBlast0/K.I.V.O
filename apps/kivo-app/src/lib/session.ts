/** How the runtime's state reads in the UI: one place for the words used by Home and the sidebar. */
import i18n from "../i18n";
import type { Link, PermissionMode, SessionState } from "../ipc/generated";

/** The permission modes in menu order (SECURITY §1.1). Bypass opens its confirmation step. */
const MODE_ORDER: ReadonlyArray<PermissionMode> = ["ask", "accept-edits", "plan", "auto", "bypass"];

/** The permission modes as the user sees them, translated. */
export function modes(): ReadonlyArray<{ value: PermissionMode; label: string }> {
  return MODE_ORDER.map((value) => ({
    value,
    label: value === "bypass" ? i18n.t("mode.bypassMenu") : i18n.t(`mode.${value}`),
  }));
}

export function modeLabel(mode: PermissionMode): string {
  return i18n.t(`mode.${mode}`);
}

export interface StateText {
  title: string;
  detail: string;
}

export type LinkTone = "ok" | "busy" | "off";

export interface LinkView extends StateText {
  /** For the sidebar: "Connected · Ready". */
  status: string;
  tone: LinkTone;
  session: SessionState | null;
}

/** What to show for the connection and state. `link` is null outside the KIVO app. */
export function viewLink(link: Link | null): LinkView {
  const t = i18n.t.bind(i18n);
  if (!link) {
    return {
      title: t("link.preview.title"),
      detail: t("link.preview.detail"),
      status: t("link.preview.status"),
      tone: "off",
      session: null,
    };
  }
  switch (link.status) {
    case "connecting":
      return {
        title: t("link.connecting.title"),
        detail: link.message ?? t("link.connecting.detail"),
        status: t("link.connecting.status"),
        tone: "busy",
        session: null,
      };
    case "reconnecting":
      return {
        title: t("link.reconnecting.title"),
        detail: t("link.reconnecting.detail"),
        status: t("link.reconnecting.status"),
        tone: "busy",
        session: null,
      };
    case "incompatible":
      return {
        title: t("link.incompatible.title"),
        detail: link.message ?? t("link.incompatible.detail"),
        status: t("link.incompatible.status"),
        tone: "off",
        session: null,
      };
    case "connected": {
      const session = link.snapshot?.session ?? "idle";
      return {
        title: t(`session.${session}.title`),
        detail: t(`session.${session}.detail`),
        status: t("link.connected", { state: t(`session.${session}.short`) }),
        tone: "ok",
        session,
      };
    }
  }
}
