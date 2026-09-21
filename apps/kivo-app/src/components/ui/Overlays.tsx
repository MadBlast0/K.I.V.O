/** Tabs, tooltip, dialog, side sheet and popover (Base UI). */
import { Dialog as BDialog } from "@base-ui/react/dialog";
import { Popover as BPopover } from "@base-ui/react/popover";
import { Tabs as BTabs } from "@base-ui/react/tabs";
import { Tooltip as BTooltip } from "@base-ui/react/tooltip";
import type { ReactElement, ReactNode } from "react";
import { IconButton } from "./Button";
import { useTranslation } from "react-i18next";

/* ───────── Page tabs with a sliding underline ───────── */
export interface TabDef<T extends string> {
  value: T;
  label: ReactNode;
  content: ReactNode;
}

export function PageTabs<T extends string>({
  tabs,
  value,
  defaultValue,
  onChange,
  label,
}: {
  tabs: ReadonlyArray<TabDef<T>>;
  value?: T;
  defaultValue?: T;
  onChange?: (v: T) => void;
  label: string;
}) {
  return (
    <BTabs.Root
      value={value}
      defaultValue={defaultValue ?? tabs[0]?.value}
      onValueChange={(v) => {
        const tab = tabs.find((t) => t.value === v);
        if (tab) onChange?.(tab.value);
      }}
    >
      <BTabs.List className="k-tabs__list" aria-label={label}>
        {tabs.map((t) => (
          <BTabs.Tab key={t.value} value={t.value} className="k-tabs__tab">
            {t.label}
          </BTabs.Tab>
        ))}
        <BTabs.Indicator className="k-tabs__indicator" />
      </BTabs.List>
      {tabs.map((t) => (
        <BTabs.Panel key={t.value} value={t.value} className="k-tabs__panel">
          {t.content}
        </BTabs.Panel>
      ))}
    </BTabs.Root>
  );
}

/* ───────── Tooltip ───────── */
export const TooltipProvider = ({ children }: { children: ReactNode }) => (
  <BTooltip.Provider delay={400}>{children}</BTooltip.Provider>
);

export function Tooltip({ content, children }: { content: ReactNode; children: ReactElement }) {
  return (
    <BTooltip.Root>
      <BTooltip.Trigger render={children} />
      <BTooltip.Portal>
        <BTooltip.Positioner sideOffset={8}>
          <BTooltip.Popup className="k-tooltip">{content}</BTooltip.Popup>
        </BTooltip.Positioner>
      </BTooltip.Portal>
    </BTooltip.Root>
  );
}

/* ───────── Dialog ───────── */
export interface DialogProps {
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  /** Element that opens the dialog. */
  trigger?: ReactElement;
  title: ReactNode;
  description?: ReactNode;
  children?: ReactNode;
  /** Footer actions; wrap buttons that should close it in <DialogClose>. */
  footer?: ReactNode;
  /** Hide the × button (for decisions that must be answered). */
  noClose?: boolean;
}

export function Dialog({ open, onOpenChange, trigger, title, description, children, footer, noClose }: DialogProps) {
  const { t } = useTranslation();
  return (
    <BDialog.Root open={open} onOpenChange={onOpenChange}>
      {trigger && <BDialog.Trigger render={trigger} />}
      <BDialog.Portal>
        <BDialog.Backdrop className="k-backdrop" />
        <BDialog.Popup className="k-dialog">
          <div className="k-dialog__head">
            <div>
              <BDialog.Title className="k-dialog__title">{title}</BDialog.Title>
              {description && <BDialog.Description className="k-dialog__desc">{description}</BDialog.Description>}
            </div>
            {!noClose && <BDialog.Close render={<IconButton icon="close" label={t("shell.close")} size="sm" />} />}
          </div>
          {children && <div className="k-dialog__body">{children}</div>}
          {footer && <div className="k-dialog__foot">{footer}</div>}
        </BDialog.Popup>
      </BDialog.Portal>
    </BDialog.Root>
  );
}

/** Makes any element close the surrounding Dialog or Sheet. */
export const DialogClose = ({ children }: { children: ReactElement }) => <BDialog.Close render={children} />;

/* ───────── Side sheet (details without leaving the page) ───────── */
export function Sheet({
  open,
  onOpenChange,
  trigger,
  title,
  children,
}: Omit<DialogProps, "description" | "footer" | "noClose">) {
  const { t } = useTranslation();
  return (
    <BDialog.Root open={open} onOpenChange={onOpenChange}>
      {trigger && <BDialog.Trigger render={trigger} />}
      <BDialog.Portal>
        <BDialog.Backdrop className="k-backdrop" />
        <BDialog.Popup className="k-sheet">
          <div className="k-sheet__head">
            <BDialog.Title className="k-sheet__title">{title}</BDialog.Title>
            <BDialog.Close render={<IconButton icon="close" label={t("shell.close")} size="sm" />} />
          </div>
          <div className="k-sheet__body">{children}</div>
        </BDialog.Popup>
      </BDialog.Portal>
    </BDialog.Root>
  );
}

/* ───────── Popover ───────── */
export function Popover({
  trigger,
  title,
  children,
  side = "bottom",
}: {
  trigger: ReactElement;
  title?: ReactNode;
  children: ReactNode;
  side?: "top" | "bottom" | "left" | "right";
}) {
  return (
    <BPopover.Root>
      <BPopover.Trigger render={trigger} />
      <BPopover.Portal>
        <BPopover.Positioner sideOffset={8} side={side}>
          <BPopover.Popup className="k-popup k-popover">
            {title && <BPopover.Title className="k-popover__title">{title}</BPopover.Title>}
            <div className="k-popover__desc">{children}</div>
          </BPopover.Popup>
        </BPopover.Positioner>
      </BPopover.Portal>
    </BPopover.Root>
  );
}
