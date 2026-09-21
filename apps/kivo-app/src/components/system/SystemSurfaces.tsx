/**
 * Previews of the Windows surfaces KIVO produces: the tray menu and tooltip, the taskbar jump
 * list, notifications, the Windows Hello prompt and the File Explorer menu. The real ones are
 * native (built in the runtime); these keep their design and wording reviewable in one place,
 * matching the mockup's System surfaces section.
 */
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon, type IconName } from "../../icons";
import { Button } from "../ui/Button";
import { TextField } from "../ui/Fields";
import { Mark } from "../ui/Status";

/* ───────── Native menus (tray, jump list, Explorer) ───────── */

/** Menu entries; each `label` is a translation key. */
export type NativeMenuEntry =
  | {
      type: "item";
      label: string;
      icon?: IconName;
      brand?: boolean;
      strong?: boolean;
      danger?: boolean;
      sub?: Array<{ label: string; checked?: boolean }>;
    }
  | { type: "separator" }
  | { type: "header"; label: string };

/** Right-click on the tray icon (UX §1). */
export const TRAY_MENU: NativeMenuEntry[] = [
  { type: "item", label: "surfaces.menu.openKivo", strong: true },
  { type: "item", label: "surfaces.menu.pauseListening", icon: "pause" },
  {
    type: "item",
    label: "surfaces.menu.permissionMode",
    icon: "permissions",
    sub: [
      { label: "surfaces.menu.askEveryTime" },
      { label: "surfaces.menu.acceptEdits" },
      { label: "surfaces.menu.planFirst" },
      { label: "surfaces.menu.auto", checked: true },
      { label: "surfaces.menu.bypassPermissions" },
    ],
  },
  { type: "item", label: "surfaces.menu.hideIslandFor1", icon: "eyeOff" },
  { type: "separator" },
  { type: "item", label: "surfaces.menu.stopEverything", icon: "stop", danger: true },
  { type: "separator" },
  { type: "item", label: "surfaces.menu.settings", icon: "settings" },
  { type: "item", label: "surfaces.menu.quitKivo", icon: "power" },
];

/** Right-click on KIVO in the taskbar (UX-58). */
export const JUMP_LIST: NativeMenuEntry[] = [
  { type: "header", label: "surfaces.menu.routines" },
  { type: "item", label: "surfaces.menu.workMode", icon: "routine" },
  { type: "item", label: "surfaces.menu.morningBrief", icon: "routine" },
  { type: "header", label: "surfaces.menu.tasks" },
  { type: "item", label: "surfaces.menu.newConversation", icon: "chat" },
  { type: "item", label: "surfaces.menu.pauseListening", icon: "pause" },
  { type: "item", label: "surfaces.menu.stopEverything", icon: "stop" },
];

/** Right-click on a file or folder in File Explorer (INT-15). */
export const EXPLORER_MENU: NativeMenuEntry[] = [
  { type: "item", label: "surfaces.menu.open", icon: "folder" },
  { type: "item", label: "surfaces.menu.copyAsPath", icon: "link" },
  { type: "separator" },
  { type: "item", label: "surfaces.menu.askKivoAboutThis", brand: true },
  { type: "item", label: "surfaces.menu.summarizeWithKivo", icon: "ai" },
  { type: "separator" },
  { type: "item", label: "surfaces.menu.rename", icon: "edit" },
];

export function NativeMenuPreview({
  entries,
  label,
  width = 250,
}: {
  entries: NativeMenuEntry[];
  label: string;
  width?: number;
}) {
  const { t } = useTranslation();
  const [open, setOpen] = useState<number | null>(null);
  return (
    <div
      className="k-native-menu"
      role="menu"
      tabIndex={-1}
      aria-label={label}
      style={{ width }}
      onMouseLeave={() => setOpen(null)}
    >
      {entries.map((e, i) => {
        if (e.type === "separator") return <div key={i} className="k-menu__separator" />;
        if (e.type === "header")
          return (
            <div key={i} className="k-menu__label">
              {t(e.label)}
            </div>
          );
        return (
          <div
            key={i}
            role="menuitem"
            tabIndex={-1}
            className={e.danger ? "k-menu__item k-menu__item--danger" : "k-menu__item"}
            data-highlighted={open === i ? "" : undefined}
            onMouseEnter={() => setOpen(e.sub ? i : null)}
            style={{ position: "relative", fontWeight: e.strong ? 600 : undefined }}
          >
            {e.brand ? <Mark size={16} /> : e.icon && <Icon name={e.icon} />}
            {t(e.label)}
            {e.sub && <Icon name="chevronRight" className="k-menu__chevron" />}
            {e.sub && open === i && (
              <div
                className="k-native-menu"
                role="menu"
                tabIndex={-1}
                style={{ position: "absolute", insetInlineStart: "100%", top: -4, width: 200, zIndex: 2 }}
              >
                {e.sub.map((s) => (
                  <div
                    key={s.label}
                    role="menuitemradio"
                    tabIndex={-1}
                    aria-checked={!!s.checked}
                    className="k-menu__item"
                  >
                    <span className="k-menu__check">{s.checked && <Icon name="check" />}</span>
                    {t(s.label)}
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/* ───────── Tray tooltip (UX-56) ───────── */
export function TrayTooltipPreview({ state, mode, tasks }: { state: string; mode: string; tasks: number }) {
  const { t } = useTranslation();
  return (
    <div className="k-tray-tip" role="tooltip">
      <div>
        <b>KIVO</b> · {state}
      </div>
      <span>{t("surfaces.tooltip", { mode, tasks })}</span>
    </div>
  );
}

/* ───────── Windows notification (UX-57, UX-58) ───────── */
const NO_ACTIONS: readonly string[] = [];

export function WindowsToastPreview({
  title,
  text,
  actions = NO_ACTIONS,
  reply = false,
}: {
  title: string;
  text: string;
  actions?: readonly string[];
  reply?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div className="k-win-toast" role="img" aria-label={t("surfaces.notification", { title })}>
      <div className="k-win-toast__head">
        <Mark size={16} />
        KIVO
        <span className="k-win-toast__x" aria-hidden>
          ✕
        </span>
      </div>
      <span className="k-win-toast__title">{title}</span>
      <span className="k-win-toast__text">{text}</span>
      {reply && (
        <div className="k-win-toast__reply">
          <TextField
            placeholder={t("surfaces.replyPlaceholder")}
            aria-label={t("surfaces.reply")}
            style={{ flex: 1 }}
          />
          <Button size="sm" variant="primary">
            {t("surfaces.send")}
          </Button>
        </div>
      )}
      {actions.length > 0 && (
        <div className="k-win-toast__actions" style={{ gridTemplateColumns: `repeat(${actions.length}, 1fr)` }}>
          {actions.map((a) => (
            <Button key={a} size="sm">
              {a}
            </Button>
          ))}
        </div>
      )}
    </div>
  );
}

/* ───────── Windows Hello prompt (SEC-11) ───────── */
export function HelloPreview({ title, detail }: { title: string; detail: string }) {
  const { t } = useTranslation();
  return (
    <div className="k-hello" role="img" aria-label={t("surfaces.hello")}>
      <div style={{ fontWeight: 600, fontSize: 15 }}>{title}</div>
      <div className="k-hello__face">
        <Icon name="user" size={26} />
      </div>
      <div style={{ color: "var(--text-2)", fontSize: 12.5 }}>{detail}</div>
      <div className="k-hello__actions">
        <Button size="sm">{t("surfaces.cancel")}</Button>
        <Button size="sm" variant="primary">
          {t("surfaces.usePin")}
        </Button>
      </div>
    </div>
  );
}
