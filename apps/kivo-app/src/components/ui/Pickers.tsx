/** Accent colour picker and keyboard-shortcut recorder. */
import { useEffect, useState } from "react";
import { ACCENTS, type Accent } from "../../lib/theme";
import { Button } from "./Button";
import { Keys } from "./Status";

export function AccentPicker({ value, onChange }: { value: Accent; onChange: (a: Accent) => void }) {
  return (
    <div className="k-swatches" role="radiogroup" aria-label="Accent colour">
      {ACCENTS.map((a) => (
        <button key={a.id} type="button" role="radio" aria-checked={value === a.id} aria-label={a.label} title={a.label}
          className="k-swatch" style={{ background: a.swatch }} onClick={() => onChange(a.id)} />
      ))}
    </div>
  );
}

const MOD_NAMES: Record<string, string> = { Control: "Ctrl", Meta: "Win", Alt: "Alt", Shift: "Shift" };

/** Click “Change”, press a combination, done. Reports conflicts via `conflict`. */
export function ShortcutRecorder({ value, onChange, conflict }: { value: string[]; onChange?: (keys: string[]) => void; conflict?: (keys: string[]) => string | null }) {
  const [recording, setRecording] = useState(false);
  const [warning, setWarning] = useState<string | null>(null);

  useEffect(() => {
    if (!recording) return;
    const onKey = (e: KeyboardEvent) => {
      e.preventDefault();
      if (e.key === "Escape") { setRecording(false); return; }
      if (e.key in MOD_NAMES) return; // wait for a non-modifier key
      const keys = [e.ctrlKey && "Ctrl", e.altKey && "Alt", e.shiftKey && "Shift", e.metaKey && "Win", e.key.length === 1 ? e.key.toUpperCase() : e.key]
        .filter(Boolean) as string[];
      setRecording(false);
      setWarning(conflict?.(keys) ?? null);
      onChange?.(keys);
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [recording, onChange, conflict]);

  return (
    <span style={{ display: "inline-flex", flexDirection: "column", alignItems: "flex-end", gap: 4 }}>
      {recording
        ? <button type="button" className="k-hotkey k-hotkey--recording" onClick={() => setRecording(false)}>Press keys… (Esc to cancel)</button>
        : <span className="k-hotkey"><Keys keys={value} /><Button size="sm" variant="plain" onClick={() => setRecording(true)}>Change</Button></span>}
      {warning && <span className="k-meta" style={{ color: "var(--orange)" }}>{warning}</span>}
    </span>
  );
}
