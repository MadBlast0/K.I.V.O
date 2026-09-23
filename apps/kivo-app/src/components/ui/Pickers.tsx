/** Accent colour picker and keyboard-shortcut recorder. */
import { useEffect, useRef, useState } from "react";
import { ACCENTS, isCustomAccent, type Accent } from "../../lib/theme";
import { Button } from "./Button";
import { Keys } from "./Status";
import { useTranslation } from "react-i18next";

/** Seven presets and an eighth, custom colour (DS-03). */
export function AccentPicker({ value, onChange }: { value: Accent; onChange: (a: Accent) => void }) {
  const { t } = useTranslation();
  const custom = isCustomAccent(value);
  const picker = useRef<HTMLInputElement>(null);
  return (
    <div className="k-swatches" role="radiogroup" aria-label={t("ui.accent")}>
      {ACCENTS.map((a) => (
        <button
          key={a.id}
          type="button"
          role="radio"
          aria-checked={value === a.id}
          aria-label={t(`accent.${a.id}`)}
          title={t(`accent.${a.id}`)}
          className="k-swatch"
          style={{ background: a.swatch }}
          onClick={() => onChange(a.id)}
        />
      ))}
      <button
        type="button"
        role="radio"
        aria-checked={custom}
        aria-label={t("accent.custom")}
        title={t("accent.custom")}
        className={custom ? "k-swatch" : "k-swatch k-swatch--custom"}
        style={custom ? { background: value } : undefined}
        onClick={() => picker.current?.click()}
      />
      <input
        ref={picker}
        type="color"
        className="k-swatch__input"
        tabIndex={-1}
        aria-hidden
        value={custom ? value : "#0a6cff"}
        onChange={(e) => {
          const hex = e.target.value.toLowerCase();
          if (isCustomAccent(hex)) onChange(hex);
        }}
      />
    </div>
  );
}

const MOD_NAMES: Record<string, string> = { Control: "Ctrl", Meta: "Win", Alt: "Alt", Shift: "Shift" };

/** Click “Change”, press a combination, done. Reports conflicts via `conflict`. */
export function ShortcutRecorder({
  value,
  onChange,
  conflict,
  onClear,
}: {
  value: string[];
  onChange?: (keys: string[]) => void;
  conflict?: (keys: string[]) => string | null;
  /** An optional shortcut: offers Remove, and says when there is none. */
  onClear?: () => void;
}) {
  const { t } = useTranslation();
  const [recording, setRecording] = useState(false);
  const [warning, setWarning] = useState<string | null>(null);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.key === "Escape") {
        setRecording(false);
        return;
      }
      if (e.key in MOD_NAMES) return; // wait for a non-modifier key
      const keys: string[] = [];
      if (e.ctrlKey) keys.push("Ctrl");
      if (e.altKey) keys.push("Alt");
      if (e.shiftKey) keys.push("Shift");
      if (e.metaKey) keys.push("Win");
      // The runtime's key names: "Space", not " " (and letters in capitals).
      keys.push(e.key === " " ? "Space" : e.key.length === 1 ? e.key.toUpperCase() : e.key);
      setRecording(false);
      setWarning(conflict?.(keys) ?? null);
      onChange?.(keys);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, onChange, conflict]);

  return (
    <span style={{ display: "inline-flex", flexDirection: "column", alignItems: "flex-end", gap: 4 }}>
      {recording ? (
        <button type="button" className="k-hotkey k-hotkey--recording" onClick={() => setRecording(false)}>
          {t("ui.pressKeys")}
        </button>
      ) : (
        <span className="k-hotkey">
          {value.length > 0 ? <Keys keys={value} /> : <span className="k-meta">{t("ui.noShortcut")}</span>}
          <Button size="sm" variant="plain" onClick={() => setRecording(true)}>
            {value.length > 0 ? t("ui.change") : t("ui.set")}
          </Button>
          {onClear && value.length > 0 && (
            <Button size="sm" variant="plain" onClick={onClear}>
              {t("ui.remove")}
            </Button>
          )}
        </span>
      )}
      {warning && (
        <span className="k-meta" style={{ color: "var(--orange)" }}>
          {warning}
        </span>
      )}
    </span>
  );
}
