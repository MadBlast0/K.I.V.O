/** Form controls built on Base UI: switch, checkbox, radio, segmented control, slider. */
import { Checkbox as BCheckbox } from "@base-ui/react/checkbox";
import { Radio as BRadio } from "@base-ui/react/radio";
import { RadioGroup as BRadioGroup } from "@base-ui/react/radio-group";
import { Slider as BSlider } from "@base-ui/react/slider";
import { Switch as BSwitch } from "@base-ui/react/switch";
import { Toggle } from "@base-ui/react/toggle";
import { ToggleGroup } from "@base-ui/react/toggle-group";
import type { ReactNode } from "react";
import { Icon } from "../../icons";

export function Switch({ checked, defaultChecked, onChange, label, disabled }: {
  checked?: boolean; defaultChecked?: boolean; onChange?: (v: boolean) => void; label: string; disabled?: boolean;
}) {
  return (
    <BSwitch.Root className="k-switch" checked={checked} defaultChecked={defaultChecked} disabled={disabled}
      onCheckedChange={(v) => onChange?.(v)} aria-label={label}>
      <BSwitch.Thumb className="k-switch__thumb" />
    </BSwitch.Root>
  );
}

export function Checkbox({ checked, defaultChecked, onChange, children }: {
  checked?: boolean; defaultChecked?: boolean; onChange?: (v: boolean) => void; children?: ReactNode;
}) {
  return (
    <label className="k-label">
      <BCheckbox.Root className="k-check" checked={checked} defaultChecked={defaultChecked} onCheckedChange={(v) => onChange?.(v)}>
        <BCheckbox.Indicator><Icon name="check" /></BCheckbox.Indicator>
      </BCheckbox.Root>
      {children}
    </label>
  );
}

export function RadioGroup<T extends string>({ value, defaultValue, onChange, children, label }: {
  value?: T; defaultValue?: T; onChange?: (v: T) => void; children: ReactNode; label: string;
}) {
  return (
    <BRadioGroup value={value} defaultValue={defaultValue} onValueChange={(v) => onChange?.(v as T)} aria-label={label}
      style={{ display: "flex", flexDirection: "column", gap: 8 }}>
      {children}
    </BRadioGroup>
  );
}

/** A bare radio circle; use inside RadioGroup. */
export function Radio({ value, children }: { value: string; children?: ReactNode }) {
  return (
    <label className="k-label">
      <BRadio.Root value={value} className="k-radio"><BRadio.Indicator className="k-radio__dot" /></BRadio.Root>
      {children}
    </label>
  );
}

/** A selectable card with title, description and optional badge; use inside RadioGroup. */
export function OptionCard({ value, title, description, badge }: { value: string; title: ReactNode; description?: ReactNode; badge?: ReactNode }) {
  return (
    <BRadio.Root value={value} className="k-option" nativeButton render={<button type="button" />}>
      <span className="k-radio" aria-hidden><BRadio.Indicator className="k-radio__dot" /></span>
      <span className="k-option__body">
        <span className="k-option__title">{title}{badge}</span>
        {description && <span className="k-option__desc">{description}</span>}
      </span>
    </BRadio.Root>
  );
}

export interface SegmentOption<T extends string> { value: T; label: ReactNode }

/** Single-choice segmented control (Base UI ToggleGroup). */
export function Segmented<T extends string>({ options, value, defaultValue, onChange, label }: {
  options: ReadonlyArray<SegmentOption<T>>; value?: T; defaultValue?: T; onChange?: (v: T) => void; label: string;
}) {
  return (
    <ToggleGroup className="k-seg" aria-label={label}
      value={value !== undefined ? [value] : undefined}
      defaultValue={defaultValue !== undefined ? [defaultValue] : undefined}
      onValueChange={(vals) => { const v = vals[0]; if (v !== undefined) onChange?.(v as T); }}>
      {options.map((o) => <Toggle key={o.value} value={o.value} className="k-seg__item">{o.label}</Toggle>)}
    </ToggleGroup>
  );
}

export function Slider({ value, defaultValue = 50, onChange, label, min = 0, max = 100 }: {
  value?: number; defaultValue?: number; onChange?: (v: number) => void; label: string; min?: number; max?: number;
}) {
  return (
    <BSlider.Root className="k-slider" value={value} defaultValue={defaultValue} min={min} max={max}
      onValueChange={(v) => onChange?.(Array.isArray(v) ? v[0] : v)}>
      <BSlider.Control className="k-slider__control">
        <BSlider.Track className="k-slider__track">
          <BSlider.Indicator className="k-slider__indicator" />
          <BSlider.Thumb className="k-slider__thumb" aria-label={label} />
        </BSlider.Track>
      </BSlider.Control>
    </BSlider.Root>
  );
}
