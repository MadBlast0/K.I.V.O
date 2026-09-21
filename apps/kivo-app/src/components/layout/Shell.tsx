/** Window shell: grouped sidebar navigation plus the page area. */
import type { ReactNode } from "react";
import { Icon, type IconName } from "../../icons";
import { cn } from "../../lib/cn";
import { Keys, Mark } from "../ui/Status";
import { TitleBar } from "./TitleBar";

export type PageId =
  | "home" | "chat" | "tasks" | "activity" | "routines"
  | "brains" | "agents" | "voice" | "extensions"
  | "permissions" | "memory" | "usage" | "settings" | "gallery";

export interface NavItem { id: PageId; label: string; icon: IconName; badge?: ReactNode }

export const NAV: NavItem[][] = [
  [
    { id: "home", label: "Home", icon: "home" },
    { id: "chat", label: "Chat", icon: "chat" },
    { id: "tasks", label: "Tasks", icon: "tasks" },
    { id: "activity", label: "Activity", icon: "activity" },
    { id: "routines", label: "Routines", icon: "routine" },
  ],
  [
    { id: "brains", label: "Brains", icon: "brain" },
    { id: "agents", label: "Agents", icon: "agent" },
    { id: "voice", label: "Voice", icon: "voice" },
    { id: "extensions", label: "Extensions", icon: "extensions" },
  ],
  [
    { id: "permissions", label: "Permissions", icon: "permissions" },
    { id: "memory", label: "Memory", icon: "memory" },
    { id: "usage", label: "Usage", icon: "usage" },
  ],
];

export function Sidebar({ current, onNavigate, onSearch, footer }: {
  current: PageId; onNavigate: (id: PageId) => void; onSearch?: () => void; footer?: ReactNode;
}) {
  return (
    <nav className="k-side" aria-label="Main">
      {/* Doubles as the left half of the title bar: drag here to move the window. */}
      <div className="k-side__brand" data-tauri-drag-region><Mark size={22} /><span>KIVO</span></div>
      {onSearch && (
        <button type="button" className="k-side__search" onClick={onSearch}>
          <Icon name="search" />Search<span style={{ marginLeft: "auto" }}><Keys keys={["Ctrl", "K"]} /></span>
        </button>
      )}
      {NAV.map((group, g) => (
        <div key={g} className="k-side__group">
          {group.map((item) => <NavButton key={item.id} item={item} current={current} onNavigate={onNavigate} />)}
        </div>
      ))}
      <div className="k-side__spacer" />
      <NavButton item={{ id: "settings", label: "Settings", icon: "settings" }} current={current} onNavigate={onNavigate} />
      {footer}
    </nav>
  );
}

function NavButton({ item, current, onNavigate }: { item: NavItem; current: PageId; onNavigate: (id: PageId) => void }) {
  const active = item.id === current;
  return (
    <button type="button" className={cn("k-side__item", active && "is-active")} aria-current={active ? "page" : undefined} onClick={() => onNavigate(item.id)}>
      <Icon name={item.icon} />{item.label}{item.badge && <span className="k-side__badge">{item.badge}</span>}
    </button>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: ReactNode; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <header className="k-page__header">
      <div><h1 className="k-page__title">{title}</h1>{subtitle && <p className="k-page__subtitle">{subtitle}</p>}</div>
      {actions && <div className="k-page__actions">{actions}</div>}
    </header>
  );
}

export function AppWindow({ sidebar, children }: { sidebar: ReactNode; children: ReactNode }) {
  return (
    <div className="k-app">
      {sidebar}
      <main className="k-page">
        <TitleBar />
        <div className="k-page__scroll"><div className="k-page__inner k-stagger">{children}</div></div>
      </main>
    </div>
  );
}
