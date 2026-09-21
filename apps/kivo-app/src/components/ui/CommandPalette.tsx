/** Ctrl+K command palette: black, top-centre, like the Island expanding. */
import { Dialog as BDialog } from "@base-ui/react/dialog";
import { useEffect, useMemo, useState } from "react";
import { Icon, type IconName } from "../../icons";

export interface Command { id: string; group: string; label: string; icon: IconName; hint?: string; run: () => void }

export function CommandPalette({ commands, open, onOpenChange }: { commands: Command[]; open: boolean; onOpenChange: (o: boolean) => void }) {
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const results = useMemo(() => commands.filter((c) => c.label.toLowerCase().includes(query.toLowerCase())), [commands, query]);

  useEffect(() => { setIndex(0); }, [query]);
  useEffect(() => { if (!open) setQuery(""); }, [open]);

  const run = (c?: Command) => { if (!c) return; onOpenChange(false); c.run(); };
  let lastGroup = "";

  return (
    <BDialog.Root open={open} onOpenChange={onOpenChange}>
      <BDialog.Portal>
        <BDialog.Backdrop className="k-backdrop" />
        <BDialog.Popup className="k-palette" aria-label="Search KIVO">
          <div className="k-palette__input">
            <Icon name="search" />
            <input autoFocus value={query} onChange={(e) => setQuery(e.target.value)} placeholder="Search settings, run routines, ask KIVO…"
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") { e.preventDefault(); setIndex((i) => Math.min(i + 1, results.length - 1)); }
                if (e.key === "ArrowUp") { e.preventDefault(); setIndex((i) => Math.max(i - 1, 0)); }
                if (e.key === "Enter") { e.preventDefault(); run(results[index]); }
              }} />
          </div>
          <div role="listbox">
            {results.length === 0 && query && (
              <><div className="k-palette__group">Ask KIVO</div><div className="k-palette__item" data-selected="true"><Icon name="ai" />“{query}”<span className="k-palette__hint">↵</span></div></>
            )}
            {results.map((c, i) => {
              const header = c.group !== lastGroup ? <div className="k-palette__group">{(lastGroup = c.group)}</div> : null;
              return (
                <div key={c.id}>
                  {header}
                  <div role="option" aria-selected={i === index} data-selected={i === index} className="k-palette__item"
                    onMouseEnter={() => setIndex(i)} onClick={() => run(c)}>
                    <Icon name={c.icon} />{c.label}{c.hint && <span className="k-palette__hint">{c.hint}</span>}
                  </div>
                </div>
              );
            })}
          </div>
          <div className="k-palette__foot"><span>↑↓ move</span><span>↵ open</span><span>Esc close</span></div>
        </BDialog.Popup>
      </BDialog.Portal>
    </BDialog.Root>
  );
}

/** Opens the palette on Ctrl+K / Cmd+K. */
export function useCommandPaletteHotkey(setOpen: (o: boolean) => void) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") { e.preventDefault(); setOpen(true); }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setOpen]);
}
