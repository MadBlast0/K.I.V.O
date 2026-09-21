/** Text inputs, search and select. */
import { Select as BSelect } from "@base-ui/react/select";
import type { InputHTMLAttributes, TextareaHTMLAttributes } from "react";
import { Icon, type IconName } from "../../icons";
import { cn } from "../../lib/cn";

export function TextField({ icon, prefix, className, style, ...input }: InputHTMLAttributes<HTMLInputElement> & { icon?: IconName; prefix?: string }) {
  return (
    <label className={cn("k-field", className)} style={style}>
      {icon && <Icon name={icon} />}
      {prefix && <span>{prefix}</span>}
      <input {...input} />
    </label>
  );
}

export function SearchField(props: InputHTMLAttributes<HTMLInputElement>) {
  return <TextField icon="search" type="search" {...props} />;
}

export function TextArea({ className, ...rest }: TextareaHTMLAttributes<HTMLTextAreaElement>) {
  return <textarea className={cn("k-textarea", className)} {...rest} />;
}

export interface SelectItem<T extends string> { value: T; label: string }

export function Select<T extends string>({ items, value, defaultValue, onChange, label, icon }: {
  items: ReadonlyArray<SelectItem<T>>; value?: T; defaultValue?: T; onChange?: (v: T) => void; label: string; icon?: IconName;
}) {
  return (
    <BSelect.Root items={items as SelectItem<T>[]} value={value} defaultValue={defaultValue} onValueChange={(v) => v != null && onChange?.(v as T)}>
      <BSelect.Trigger className="k-select" aria-label={label}>
        {icon && <Icon name={icon} size={14} />}
        <BSelect.Value />
        <BSelect.Icon><Icon name="chevronDown" className="k-select__icon" /></BSelect.Icon>
      </BSelect.Trigger>
      <BSelect.Portal>
        <BSelect.Positioner sideOffset={6} alignItemWithTrigger={false}>
          <BSelect.Popup className="k-popup">
            <BSelect.List>
              {items.map((it) => (
                <BSelect.Item key={it.value} value={it.value} className="k-menu__item">
                  <span className="k-menu__check"><BSelect.ItemIndicator><Icon name="check" /></BSelect.ItemIndicator></span>
                  <BSelect.ItemText>{it.label}</BSelect.ItemText>
                </BSelect.Item>
              ))}
            </BSelect.List>
          </BSelect.Popup>
        </BSelect.Positioner>
      </BSelect.Portal>
    </BSelect.Root>
  );
}
