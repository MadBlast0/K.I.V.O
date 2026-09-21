/** macOS-style grouped lists: Section heading, Group container, Row. */
import type { ReactNode } from "react";
import { Icon, type IconName } from "../../icons";
import { cn } from "../../lib/cn";

export function Section({ title, aside }: { title: ReactNode; aside?: ReactNode }) {
  return <div className="k-section"><span>{title}</span>{aside && <span className="k-section__aside">{aside}</span>}</div>;
}

export function Group({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("k-group", className)} role="list">{children}</div>;
}

export interface RowProps {
  /** A semantic icon name, or any leading element (monogram, spinner, status). */
  lead?: IconName | ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  end?: ReactNode;
  /** Shows a chevron and makes the row clickable. */
  onClick?: () => void;
  chevron?: boolean;
}

export function Row({ lead, title, subtitle, end, onClick, chevron = !!onClick }: RowProps) {
  const hasLead = lead !== undefined && lead !== null;
  const leadEl = typeof lead === "string" ? <span className="k-row__icon"><Icon name={lead as IconName} /></span> : lead;
  return (
    <div role="listitem" className={cn("k-row", hasLead && "k-row--icon", onClick && "k-row--clickable")}
      onClick={onClick} tabIndex={onClick ? 0 : undefined} onKeyDown={onClick ? (e) => { if (e.key === "Enter") onClick(); } : undefined}>
      {hasLead && leadEl}
      <div className="k-row__text">
        <div className="k-row__title">{title}</div>
        {subtitle && <div className="k-row__subtitle">{subtitle}</div>}
      </div>
      {(end || chevron) && <div className="k-row__end">{end}{chevron && <Icon name="chevronRight" className="k-row__chevron" />}</div>}
    </div>
  );
}

export const Note = ({ children }: { children: ReactNode }) => <p className="k-note">{children}</p>;
export const Meta = ({ children }: { children: ReactNode }) => <span className="k-meta">{children}</span>;
