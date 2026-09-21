/**
 * In-app previews of Windows surfaces KIVO produces: the tray menu, a notification toast and
 * the Windows Hello prompt. The real ones are native (Rust side); these show users, in
 * Settings, what to expect, and let us review the wording in one place.
 */
import { useState } from "react";
import { Icon, type IconName } from "../../icons";
import { Button } from "../ui/Button";
import { Keys, Mark } from "../ui/Status";

/* ───────── Tray menu (with a hover submenu) ───────── */
interface TrayItem { label: string; icon?: IconName; keys?: string[]; sub?: string[]; danger?: boolean; sep?: boolean }

export const TRAY_MENU: TrayItem[] = [
  { label: "Talk to KIVO", icon: "mic", keys: ["Ctrl", "Space"] },
  { label: "Type to KIVO", icon: "keyboard", keys: ["Ctrl", "Shift", "Space"] },
  { label: "", sep: true },
  { label: "Pause listening", icon: "pause", sub: ["For 15 minutes", "For 1 hour", "Until I turn it back on"] },
  { label: "Permission mode", icon: "permissions", sub: ["Ask every time", "Accept edits", "Plan first", "✓ Auto", "Bypass permissions"] },
  { label: "", sep: true },
  { label: "Open KIVO", icon: "window" },
  { label: "Settings", icon: "settings" },
  { label: "", sep: true },
  { label: "Stop everything", icon: "stop", keys: ["Ctrl", "Alt", "Shift", "Esc"], danger: true },
  { label: "Quit KIVO", icon: "power" },
];

export function TrayMenuPreview({ items = TRAY_MENU }: { items?: TrayItem[] }) {
  const [open, setOpen] = useState<number | null>(null);
  return (
    <div className="k-native-menu" role="menu" aria-label="Tray menu preview" style={{ width: 330, whiteSpace: "nowrap" }} onMouseLeave={() => setOpen(null)}>
      {items.map((it, i) => it.sep ? <div key={i} className="k-menu__separator" /> : (
        <div key={i} role="menuitem" className={it.danger ? "k-menu__item k-menu__item--danger" : "k-menu__item"}
          data-highlighted={open === i ? "" : undefined} onMouseEnter={() => setOpen(it.sub ? i : null)} style={{ position: "relative" }}>
          {it.icon && <Icon name={it.icon} />}{it.label}
          {it.keys && <span className="k-menu__shortcut"><Keys keys={it.keys} /></span>}
          {it.sub && <Icon name="chevronRight" className="k-menu__chevron" />}
          {it.sub && open === i && (
            <div className="k-native-menu" style={{ position: "absolute", left: "100%", top: -4, width: 210, zIndex: 2 }}>
              {it.sub.map((s) => <div key={s} className="k-menu__item">{s}</div>)}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}

/* ───────── Windows notification ───────── */
export function WindowsToastPreview({ title, text, actions = [] }: { title: string; text: string; actions?: string[] }) {
  return (
    <div className="k-win-toast" role="img" aria-label={`Notification: ${title}`}>
      <div className="k-win-toast__head"><Mark size={16} />KIVO<span style={{ marginLeft: "auto" }}>now</span></div>
      <span className="k-win-toast__title">{title}</span>
      <span className="k-win-toast__text">{text}</span>
      {actions.length > 0 && (
        <div className="k-win-toast__actions" style={{ gridTemplateColumns: `repeat(${actions.length}, 1fr)` }}>
          {actions.map((a) => <Button key={a} size="sm">{a}</Button>)}
        </div>
      )}
    </div>
  );
}

/* ───────── Windows Hello prompt ───────── */
export function HelloPreview({ reason }: { reason: string }) {
  return (
    <div className="k-hello" role="img" aria-label="Windows Hello prompt preview">
      <div style={{ fontSize: 12, color: "var(--text-2)" }}>Windows Security</div>
      <div style={{ fontWeight: 600, fontSize: 15, marginTop: 6 }}>Making sure it’s you</div>
      <div className="k-hello__face"><Icon name="user" size={26} /></div>
      <div style={{ color: "var(--text-2)", fontSize: 12.5 }}>KIVO wants to: {reason}</div>
      <div style={{ marginTop: 14 }}><Button size="sm">Cancel</Button></div>
    </div>
  );
}
