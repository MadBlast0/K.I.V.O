/** Window shell: grouped sidebar navigation plus the page area. */
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Icon, type IconName } from "../../icons";
import { cn } from "../../lib/cn";
import { Keys, Mark } from "../ui/Status";
import { TitleBar } from "./TitleBar";

export type PageId =
  | "home"
  | "chat"
  | "tasks"
  | "activity"
  | "routines"
  | "brains"
  | "agents"
  | "voice"
  | "extensions"
  | "permissions"
  | "memory"
  | "usage"
  | "settings"
  | "gallery";

export interface NavItem {
  id: PageId;
  icon: IconName;
  badge?: ReactNode;
}

export const NAV: NavItem[][] = [
  [
    { id: "home", icon: "home" },
    { id: "chat", icon: "chat" },
    { id: "tasks", icon: "tasks" },
    { id: "activity", icon: "activity" },
    { id: "routines", icon: "routine" },
  ],
  [
    { id: "brains", icon: "brain" },
    { id: "agents", icon: "agent" },
    { id: "voice", icon: "voice" },
    { id: "extensions", icon: "extensions" },
  ],
  [
    { id: "permissions", icon: "permissions" },
    { id: "memory", icon: "memory" },
    { id: "usage", icon: "usage" },
  ],
];

export function Sidebar({
  current,
  onNavigate,
  onSearch,
  footer,
}: {
  current: PageId;
  onNavigate: (id: PageId) => void;
  onSearch?: () => void;
  footer?: ReactNode;
}) {
  const { t } = useTranslation();
  return (
    <nav className="k-side" aria-label={t("shell.nav")}>
      {/* Doubles as the left half of the title bar: drag here to move the window. */}
      <div className="k-side__brand" data-tauri-drag-region>
        <Mark size={22} />
        <span>KIVO</span>
      </div>
      {onSearch && (
        <button type="button" className="k-side__search" onClick={onSearch}>
          <Icon name="search" />
          {t("shell.search")}
          <span style={{ marginInlineStart: "auto" }}>
            <Keys keys={["Ctrl", "K"]} />
          </span>
        </button>
      )}
      {NAV.map((group, g) => (
        <div key={g} className="k-side__group">
          {group.map((item) => (
            <NavButton key={item.id} item={item} current={current} onNavigate={onNavigate} />
          ))}
        </div>
      ))}
      <div className="k-side__spacer" />
      <NavButton item={{ id: "settings", icon: "settings" }} current={current} onNavigate={onNavigate} />
      {footer}
    </nav>
  );
}

function NavButton({
  item,
  current,
  onNavigate,
}: {
  item: NavItem;
  current: PageId;
  onNavigate: (id: PageId) => void;
}) {
  const { t } = useTranslation();
  const active = item.id === current;
  return (
    <button
      type="button"
      className={cn("k-side__item", active && "is-active")}
      aria-current={active ? "page" : undefined}
      onClick={() => onNavigate(item.id)}
    >
      <Icon name={item.icon} />
      {t(`nav.${item.id}`)}
      {item.badge && <span className="k-side__badge">{item.badge}</span>}
    </button>
  );
}

export function PageHeader({
  title,
  subtitle,
  actions,
}: {
  title: ReactNode;
  subtitle?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="k-page__header">
      <div>
        <h1 className="k-page__title">{title}</h1>
        {subtitle && <p className="k-page__subtitle">{subtitle}</p>}
      </div>
      {actions && <div className="k-page__actions">{actions}</div>}
    </header>
  );
}

/** The window: the sidebar and a scrolling page, or — without a sidebar — a full-window screen
 * that lays itself out (onboarding). */
export function AppWindow({
  sidebar,
  title,
  children,
}: {
  sidebar?: ReactNode;
  /** The page's name, shown in the title bar. */
  title?: string;
  children: ReactNode;
}) {
  if (sidebar === undefined) {
    return (
      <div className="k-app k-app--bare">
        <main className="k-page">
          <TitleBar />
          {children}
        </main>
      </div>
    );
  }
  return (
    <div className="k-app">
      {sidebar}
      <main className="k-page">
        <TitleBar title={title} />
        <div className="k-page__scroll">
          <div className="k-page__inner k-stagger">{children}</div>
        </div>
      </main>
    </div>
  );
}
