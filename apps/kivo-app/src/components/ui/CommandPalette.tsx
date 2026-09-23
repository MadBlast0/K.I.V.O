/** Ctrl+K command palette (UX-47): black, top-centre, like the Island expanding. It finds pages,
 * every setting, routines to run and actions; anything else is asked of KIVO. An ARIA combobox:
 * the input keeps focus and points at the highlighted option with aria-activedescendant. */
import { Dialog as BDialog } from "@base-ui/react/dialog";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import { Icon, type IconName } from "../../icons";
import { useTranslation } from "react-i18next";

export interface Command {
  id: string;
  group: string;
  label: string;
  icon: IconName;
  hint?: string;
  /** More words it's found by. */
  keywords?: string;
  run: () => void;
}

export function CommandPalette({
  commands,
  open,
  onOpenChange,
  onAsk,
}: {
  commands: Command[];
  open: boolean;
  onOpenChange: (o: boolean) => void;
  /** What wasn't found goes to KIVO as a typed request. */
  onAsk?: (text: string) => void;
}) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [index, setIndex] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const listId = useId();

  const results = useMemo(() => {
    const words = query.toLowerCase().split(/\s+/).filter(Boolean);
    return commands.filter((c) => {
      const text = `${c.label} ${c.group} ${c.keywords ?? ""}`.toLowerCase();
      return words.every((w) => text.includes(w));
    });
  }, [commands, query]);
  // Group headers are derived up front, not while rendering the rows.
  const rows = useMemo(
    () =>
      results.map((c, i) => ({ command: c, header: i === 0 || results[i - 1]?.group !== c.group ? c.group : null })),
    [results],
  );

  const setOpen = (next: boolean) => {
    if (!next) {
      setQuery("");
      setIndex(0);
    }
    onOpenChange(next);
  };
  const run = (c?: Command) => {
    if (!c) return;
    setOpen(false);
    c.run();
  };
  const optionId = (i: number) => `${listId}-${i}`;

  return (
    <BDialog.Root open={open} onOpenChange={setOpen}>
      <BDialog.Portal>
        <BDialog.Backdrop className="k-backdrop" />
        <BDialog.Popup className="k-palette" aria-label={t("palette.label")} initialFocus={input}>
          <div className="k-palette__input">
            <Icon name="search" />
            <input
              ref={input}
              value={query}
              placeholder={t("palette.placeholder")}
              role="combobox"
              aria-expanded
              aria-controls={listId}
              aria-autocomplete="list"
              aria-activedescendant={results.length ? optionId(index) : undefined}
              onChange={(e) => {
                setQuery(e.target.value);
                setIndex(0);
              }}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  setIndex((i) => Math.min(i + 1, results.length - 1));
                }
                if (e.key === "ArrowUp") {
                  e.preventDefault();
                  setIndex((i) => Math.max(i - 1, 0));
                }
                if (e.key === "Enter") {
                  e.preventDefault();
                  if (results.length === 0 && query.trim() && onAsk) {
                    setOpen(false);
                    onAsk(query.trim());
                  } else {
                    run(results[index]);
                  }
                }
              }}
            />
          </div>
          <div role="listbox" id={listId} aria-label={t("palette.results")}>
            {results.length === 0 && query && (
              <>
                <div className="k-palette__group">{t("palette.ask")}</div>
                <div
                  role="option"
                  tabIndex={-1}
                  aria-selected
                  className="k-palette__item"
                  data-selected="true"
                  onClick={() => {
                    if (!onAsk) return;
                    setOpen(false);
                    onAsk(query.trim());
                  }}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" && onAsk) {
                      setOpen(false);
                      onAsk(query.trim());
                    }
                  }}
                >
                  <Icon name="ai" />“{query}”<span className="k-palette__hint">↵</span>
                </div>
              </>
            )}
            {rows.map(({ command: c, header }, i) => (
              <div key={c.id}>
                {header && (
                  <div className="k-palette__group" aria-hidden>
                    {header}
                  </div>
                )}
                <div
                  id={optionId(i)}
                  role="option"
                  tabIndex={-1}
                  aria-selected={i === index}
                  data-selected={i === index}
                  className="k-palette__item"
                  onMouseEnter={() => setIndex(i)}
                  onClick={() => run(c)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") run(c);
                  }}
                >
                  <Icon name={c.icon} />
                  {c.label}
                  {c.hint && <span className="k-palette__hint">{c.hint}</span>}
                </div>
              </div>
            ))}
          </div>
          <div className="k-palette__foot">
            <span>{t("palette.move")}</span>
            <span>{t("palette.open")}</span>
            <span>{t("palette.close")}</span>
          </div>
        </BDialog.Popup>
      </BDialog.Portal>
    </BDialog.Root>
  );
}

/** Opens the palette on Ctrl+K / Cmd+K. */
export function useCommandPaletteHotkey(setOpen: (o: boolean) => void) {
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen(true);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setOpen]);
}
