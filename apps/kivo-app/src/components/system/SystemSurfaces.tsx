/**
 * Previews of the Windows surfaces KIVO produces: the tray menu and tooltip, the taskbar jump
 * list, notifications, the Windows Hello prompt and the File Explorer menu. The real ones are
 * native (built in the runtime); these keep their design and wording reviewable in one place,
 * matching the mockup's System surfaces section.
 */
import { useState } from "react";
import { Icon, type IconName } from "../../icons";
import { Button } from "../ui/Button";
import { TextField } from "../ui/Fields";
import { Mark } from "../ui/Status";

/* ───────── Native menus (tray, jump list, Explorer) ───────── */

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
  { type: "item", label: "Open KIVO", strong: true },
  { type: "item", label: "Pause listening", icon: "pause" },
  {
    type: "item",
    label: "Permission mode",
    icon: "permissions",
    sub: [
      { label: "Ask every time" },
      { label: "Accept edits" },
      { label: "Plan first" },
      { label: "Auto", checked: true },
      { label: "Bypass permissions" },
    ],
  },
  { type: "item", label: "Hide Island for 1 hour", icon: "eyeOff" },
  { type: "separator" },
  { type: "item", label: "Stop everything", icon: "stop", danger: true },
  { type: "separator" },
  { type: "item", label: "Settings", icon: "settings" },
  { type: "item", label: "Quit KIVO", icon: "power" },
];

/** Right-click on KIVO in the taskbar (UX-58). */
export const JUMP_LIST: NativeMenuEntry[] = [
  { type: "header", label: "Routines" },
  { type: "item", label: "Work mode", icon: "routine" },
  { type: "item", label: "Morning brief", icon: "routine" },
  { type: "header", label: "Tasks" },
  { type: "item", label: "New conversation", icon: "chat" },
  { type: "item", label: "Pause listening", icon: "pause" },
  { type: "item", label: "Stop everything", icon: "stop" },
];

/** Right-click on a file or folder in File Explorer (INT-15). */
export const EXPLORER_MENU: NativeMenuEntry[] = [
  { type: "item", label: "Open", icon: "folder" },
  { type: "item", label: "Copy as path", icon: "link" },
  { type: "separator" },
  { type: "item", label: "Ask KIVO about this", brand: true },
  { type: "item", label: "Summarize with KIVO", icon: "ai" },
  { type: "separator" },
  { type: "item", label: "Rename", icon: "edit" },
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
              {e.label}
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
            {e.label}
            {e.sub && <Icon name="chevronRight" className="k-menu__chevron" />}
            {e.sub && open === i && (
              <div
                className="k-native-menu"
                role="menu"
                tabIndex={-1}
                style={{ position: "absolute", left: "100%", top: -4, width: 200, zIndex: 2 }}
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
                    {s.label}
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
  return (
    <div className="k-tray-tip" role="tooltip">
      <div>
        <b>KIVO</b> · {state}
      </div>
      <span>
        {mode} mode · {tasks === 1 ? "1 task running" : `${tasks} tasks running`}
      </span>
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
  return (
    <div className="k-win-toast" role="img" aria-label={`Notification: ${title}`}>
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
          <TextField placeholder="Reply to KIVO…" aria-label="Reply to KIVO" style={{ flex: 1 }} />
          <Button size="sm" variant="primary">
            Send
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
  return (
    <div className="k-hello" role="img" aria-label="Windows Hello prompt preview">
      <div style={{ fontWeight: 600, fontSize: 15 }}>{title}</div>
      <div className="k-hello__face">
        <Icon name="user" size={26} />
      </div>
      <div style={{ color: "var(--text-2)", fontSize: 12.5 }}>{detail}</div>
      <div className="k-hello__actions">
        <Button size="sm">Cancel</Button>
        <Button size="sm" variant="primary">
          Use PIN
        </Button>
      </div>
    </div>
  );
}
