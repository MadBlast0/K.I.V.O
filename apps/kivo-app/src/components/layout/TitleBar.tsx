/**
 * KIVO draws its own title bar (the native one is off: `decorations: false`), so the header
 * and the window buttons are one surface. Empty areas carrying `data-tauri-drag-region` move
 * the window and double-click to maximize, like a native caption.
 */
import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useState } from "react";

// Segoe Fluent Icons (Windows 11) with Segoe MDL2 Assets (Windows 10) as the fallback,
// so the buttons match the system caption buttons exactly.
const GLYPH = { minimize: "\uE921", maximize: "\uE922", restore: "\uE923", close: "\uE8BB" };

/** Window commands are fire-and-forget; a failure (e.g. a missing permission) is logged, not thrown. */
const run = (action: Promise<unknown>) => {
  action.catch((error: unknown) => console.error("window command failed", error));
};

export function WindowControls() {
  const [maximized, setMaximized] = useState(false);

  useEffect(() => {
    if (!isTauri()) return;
    const win = getCurrentWindow();
    let unlisten: (() => void) | undefined;
    const sync = () => {
      win
        .isMaximized()
        .then(setMaximized)
        .catch(() => {});
    };
    sync();
    win
      .onResized(sync)
      .then((u) => {
        unlisten = u;
      })
      .catch(() => {});
    return () => unlisten?.();
  }, []);

  // In a plain browser (UI development without Tauri) there is no window to control.
  if (!isTauri()) return null;
  const win = getCurrentWindow();

  return (
    <div className="k-caption" role="group" aria-label="Window">
      <button
        type="button"
        className="k-caption__btn"
        aria-label="Minimize"
        title="Minimize"
        onClick={() => run(win.minimize())}
      >
        {GLYPH.minimize}
      </button>
      <button
        type="button"
        className="k-caption__btn"
        aria-label={maximized ? "Restore" : "Maximize"}
        title={maximized ? "Restore" : "Maximize"}
        onClick={() => run(win.toggleMaximize())}
      >
        {maximized ? GLYPH.restore : GLYPH.maximize}
      </button>
      <button
        type="button"
        className="k-caption__btn k-caption__btn--close"
        aria-label="Close"
        title="Close"
        onClick={() => run(win.close())}
      >
        {GLYPH.close}
      </button>
    </div>
  );
}

/** The strip across the top of the page area: drag region plus window buttons. */
export function TitleBar() {
  return (
    <div className="k-titlebar">
      <div className="k-titlebar__drag" data-tauri-drag-region />
      <WindowControls />
    </div>
  );
}
