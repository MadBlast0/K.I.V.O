/** Status and small display pieces: tags, pills, keys, progress, marks. */
import type { ReactNode } from "react";
import { Icon } from "../../icons";
import { cn } from "../../lib/cn";
import { useTranslation } from "react-i18next";

export type Tone = "neutral" | "success" | "warning" | "danger" | "accent";

/** Dot + text. Status is never communicated by colour alone. */
export function Tag({ tone = "neutral", children }: { tone?: Tone; children: ReactNode }) {
  return (
    <span className={cn("k-tag", tone !== "neutral" && `k-tag--${tone}`)}>
      <span className="k-tag__dot" />
      {children}
    </span>
  );
}

/** Small label: risk levels, release status, "Recommended". */
export function Pill({ tone = "neutral", children }: { tone?: Tone; children: ReactNode }) {
  return <span className={cn("k-pill", tone !== "neutral" && `k-pill--${tone}`)}>{children}</span>;
}

export function Keys({ keys }: { keys: string[] }) {
  return (
    <span className="k-keys">
      {keys.map((k) => (
        <kbd key={k} className="k-kbd">
          {k}
        </kbd>
      ))}
    </span>
  );
}

export function NewDot() {
  const { t } = useTranslation();
  return <span className="k-new" role="img" title={t("ui.new")} aria-label={t("ui.new")} />;
}
export function Spinner({ label }: { label?: string }) {
  const { t } = useTranslation();
  return <span className="k-spinner" role="status" aria-label={label ?? t("ui.working")} />;
}
export function Done() {
  const { t } = useTranslation();
  return (
    <span className="k-done" role="img" aria-label={t("ui.done")}>
      <Icon name="check" />
    </span>
  );
}

export function Meter({ value, label }: { value: number; label: string }) {
  return (
    <div
      className="k-meter"
      role="progressbar"
      aria-label={label}
      aria-valuenow={value}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <span style={{ width: `${value}%` }} />
    </div>
  );
}

/** Spend against a budget, with markers at the user's warning thresholds. */
const DEFAULT_THRESHOLDS: readonly number[] = [50, 80, 95];

export function BudgetMeter({
  value,
  thresholds = DEFAULT_THRESHOLDS,
  label,
}: {
  value: number;
  thresholds?: readonly number[];
  label: string;
}) {
  return (
    <div
      className="k-budget"
      role="progressbar"
      aria-label={label}
      aria-valuenow={value}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <span style={{ width: `${Math.min(100, value)}%` }} />
      {thresholds.map((t) => (
        <em key={t} style={{ insetInlineStart: `${t}%` }} />
      ))}
    </div>
  );
}

/** Animated input-level bars (mic check, recording). */
export function LevelMeter({ bars = 14, reverse = false }: { bars?: number; reverse?: boolean }) {
  return (
    <span className="k-level" aria-hidden>
      {Array.from({ length: bars }, (_, i) => (
        <i key={i} style={{ animationDelay: `${((reverse ? bars - i : i) * 83) % 700}ms` }} />
      ))}
    </span>
  );
}

export const Orb = ({ size = 18 }: { size?: number }) => (
  <span className="k-orb" style={{ width: size, height: size }} aria-hidden />
);
/** The KIVO mark: black disc with the blue orb. */
export const Mark = ({ size = 22 }: { size?: number }) => (
  <span className="k-mark" style={{ width: size, height: size }} aria-hidden />
);

/** Monogram logo for third-party services (no trademarked logos). */
export function Monogram({ text, color, large }: { text: string; color: string; large?: boolean }) {
  return (
    <span className={cn("k-mono", large && "k-mono--lg")} style={{ background: color }} aria-hidden>
      {text}
    </span>
  );
}
