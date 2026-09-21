/** Alerts, empty states, tiles and stats. */
import type { ReactNode } from "react";
import { Icon, type IconName } from "../../icons";

export type AlertKind = "info" | "success" | "warning" | "danger";
const ALERT_ICON: Record<AlertKind, IconName> = {
  info: "info",
  success: "check",
  warning: "warning",
  danger: "warning",
};

export function Alert({
  kind = "info",
  title,
  children,
}: {
  kind?: AlertKind;
  title: ReactNode;
  children?: ReactNode;
}) {
  return (
    <div className={`k-alert k-alert--${kind}`} role={kind === "danger" ? "alert" : "status"}>
      <Icon name={ALERT_ICON[kind]} />
      <div style={{ minWidth: 0 }}>
        <span className="k-alert__title">{title}</span>
        {children}
      </div>
    </div>
  );
}

export function EmptyState({
  icon,
  title,
  children,
  action,
}: {
  icon: IconName;
  title: ReactNode;
  children?: ReactNode;
  action?: ReactNode;
}) {
  return (
    <div className="k-empty">
      <Icon name={icon} />
      <span className="k-empty__title">{title}</span>
      {children}
      {action && <div style={{ marginTop: 10 }}>{action}</div>}
    </div>
  );
}

export const Tile = ({ children, style }: { children: ReactNode; style?: React.CSSProperties }) => (
  <div className="k-tile" style={style}>
    {children}
  </div>
);

export function Stat({ value, label }: { value: ReactNode; label: ReactNode }) {
  return (
    <div className="k-tile">
      <div className="k-stat__value">{value}</div>
      <div className="k-stat__label">{label}</div>
    </div>
  );
}
