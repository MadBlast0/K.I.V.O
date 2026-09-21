/** macOS-style grouped lists: Section heading, Group container, Row. */
import type { ReactNode } from "react";
import { Icon, type IconName } from "../../icons";
import { cn } from "../../lib/cn";

export function Section({ title, aside }: { title: ReactNode; aside?: ReactNode }) {
  return (
    <div className="k-section">
      <span>{title}</span>
      {aside && <span className="k-section__aside">{aside}</span>}
    </div>
  );
}

export function Group({ children, className }: { children: ReactNode; className?: string }) {
  return <div className={cn("k-group", className)}>{children}</div>;
}

export interface RowProps {
  /** A semantic icon shown before the text. */
  icon?: IconName;
  /** Any other leading element (monogram, spinner, status); used when `icon` isn't set. */
  lead?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  end?: ReactNode;
  /** Makes the whole row a button and shows a chevron. */
  onClick?: () => void;
  chevron?: boolean;
}

export function Row({ icon, lead, title, subtitle, end, onClick, chevron = !!onClick }: RowProps) {
  const leadEl = icon ? (
    <span className="k-row__icon">
      <Icon name={icon} />
    </span>
  ) : (
    lead
  );
  const hasLead = leadEl !== undefined && leadEl !== null;
  const content = (
    <>
      {hasLead && leadEl}
      <div className="k-row__text">
        <div className="k-row__title">{title}</div>
        {subtitle && <div className="k-row__subtitle">{subtitle}</div>}
      </div>
      {(end || chevron) && (
        <div className="k-row__end">
          {end}
          {chevron && <Icon name="chevronRight" className="k-row__chevron" />}
        </div>
      )}
    </>
  );
  const className = cn("k-row", hasLead && "k-row--icon", onClick && "k-row--clickable");
  // A clickable row is a real button, so keyboard and screen-reader users get it for free.
  return onClick ? (
    <button type="button" className={className} onClick={onClick}>
      {content}
    </button>
  ) : (
    <div className={className}>{content}</div>
  );
}

export const Note = ({ children }: { children: ReactNode }) => <p className="k-note">{children}</p>;
export const Meta = ({ children }: { children: ReactNode }) => <span className="k-meta">{children}</span>;
