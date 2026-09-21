/**
 * Dropdown and right-click menus with nested submenus, checks, shortcuts and danger items.
 * Menus are described as data so the same structure can drive the in-app menu and, later,
 * the native tray menu.
 */
import { ContextMenu as BContextMenu } from "@base-ui/react/context-menu";
import { Menu as BMenu } from "@base-ui/react/menu";
import type { ReactElement, ReactNode } from "react";
import { Icon, type IconName } from "../../icons";
import { cn } from "../../lib/cn";

export type MenuEntry =
  | { type: "item"; label: ReactNode; icon?: IconName; shortcut?: string; danger?: boolean; disabled?: boolean; checked?: boolean; onSelect?: () => void }
  | { type: "submenu"; label: ReactNode; icon?: IconName; hint?: string; items: MenuEntry[] }
  | { type: "separator" }
  | { type: "label"; label: ReactNode };

// Base UI shares Menu parts between Menu and ContextMenu, so one renderer serves both.
function Entries({ items }: { items: MenuEntry[] }) {
  return (
    <>
      {items.map((e, i) => {
        if (e.type === "separator") return <BMenu.Separator key={i} className="k-menu__separator" />;
        if (e.type === "label") return <div key={i} className="k-menu__label">{e.label}</div>;
        if (e.type === "submenu") {
          return (
            <BMenu.SubmenuRoot key={i}>
              <BMenu.SubmenuTrigger className="k-menu__item" openOnHover delay={120}>
                {e.icon && <Icon name={e.icon} />}{e.label}
                {e.hint && <span className="k-menu__shortcut">{e.hint}</span>}
                <Icon name="chevronRight" className="k-menu__chevron" />
              </BMenu.SubmenuTrigger>
              <BMenu.Portal>
                <BMenu.Positioner side="right" align="start" sideOffset={4} alignOffset={-4}>
                  <BMenu.Popup className="k-popup"><Entries items={e.items} /></BMenu.Popup>
                </BMenu.Positioner>
              </BMenu.Portal>
            </BMenu.SubmenuRoot>
          );
        }
        return (
          <BMenu.Item key={i} className={cn("k-menu__item", e.danger && "k-menu__item--danger")} disabled={e.disabled} onClick={e.onSelect}>
            {e.checked !== undefined ? <span className="k-menu__check">{e.checked && <Icon name="check" />}</span> : e.icon && <Icon name={e.icon} />}
            {e.label}
            {e.shortcut && <span className="k-menu__shortcut">{e.shortcut}</span>}
          </BMenu.Item>
        );
      })}
    </>
  );
}

export function DropdownMenu({ trigger, items, align = "start" }: { trigger: ReactElement; items: MenuEntry[]; align?: "start" | "center" | "end" }) {
  return (
    <BMenu.Root>
      <BMenu.Trigger render={trigger} />
      <BMenu.Portal>
        <BMenu.Positioner sideOffset={6} align={align}>
          <BMenu.Popup className="k-popup"><Entries items={items} /></BMenu.Popup>
        </BMenu.Positioner>
      </BMenu.Portal>
    </BMenu.Root>
  );
}

export function ContextMenu({ children, items }: { children: ReactNode; items: MenuEntry[] }) {
  return (
    <BContextMenu.Root>
      <BContextMenu.Trigger>{children}</BContextMenu.Trigger>
      <BContextMenu.Portal>
        <BContextMenu.Positioner>
          <BContextMenu.Popup className="k-popup"><Entries items={items} /></BContextMenu.Popup>
        </BContextMenu.Positioner>
      </BContextMenu.Portal>
    </BContextMenu.Root>
  );
}
