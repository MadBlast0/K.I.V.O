/**
 * Keeps the appearance in step with the runtime (UX-37): what `kivo.toml` holds is applied, and
 * changes made in the app are saved there. Before M7 the appearance lived only in this window's
 * storage; the first time the runtime still has the defaults, the user's earlier choice is moved
 * into it instead of being overwritten.
 */
import { useEffect, useRef } from "react";
import { useSettings } from "./settings";
import { parse, useTheme, type Appearance } from "./theme";

const MOVED = "kivo.appearance.moved";

function isDefault(a: Partial<Appearance>): boolean {
  return (
    (a.theme ?? "light") === "light" &&
    (a.accent ?? "blue") === "blue" &&
    (a.textSize ?? "normal") === "normal" &&
    (a.motion ?? "system") === "system" &&
    (a.transparency ?? true)
  );
}

function moved(): boolean {
  try {
    return localStorage.getItem(MOVED) === "1";
  } catch {
    return true;
  }
}

function markMoved() {
  try {
    localStorage.setItem(MOVED, "1");
  } catch {
    /* storage unavailable */
  }
}

export function ThemeSync() {
  const theme = useTheme();
  const [settings, save] = useSettings();
  const latest = useRef(theme);
  useEffect(() => {
    latest.current = theme;
  });

  useEffect(() => {
    latest.current.setSaver(save);
    return () => latest.current.setSaver(null);
  }, [save]);

  useEffect(() => {
    const raw = settings?.appearance;
    if (!raw) return;
    const runtime = parse(raw);
    const local: Appearance = {
      theme: latest.current.theme,
      accent: latest.current.accent,
      textSize: latest.current.textSize,
      motion: latest.current.motion,
      transparency: latest.current.transparency,
    };
    if (!moved()) {
      markMoved();
      if (isDefault(runtime) && !isDefault(local)) {
        save({
          appearance: {
            theme: local.theme,
            accent: local.accent,
            "text-size": local.textSize,
            motion: local.motion,
            transparency: local.transparency,
          },
        });
        return;
      }
    }
    latest.current.apply(runtime);
  }, [settings, save]);

  return null;
}
