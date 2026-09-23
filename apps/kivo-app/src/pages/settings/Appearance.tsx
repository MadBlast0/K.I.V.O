/**
 * Settings → Appearance (UX-31, DS-03, UX-32): theme, accent colour (seven presets or custom),
 * text size, animations and transparency effects (Mica on Windows 11). The theme provider shows a
 * change at once and saves it to the runtime.
 */
import { useTranslation } from "react-i18next";
import { AccentPicker, Group, Note, Row, Section, Segmented, Switch } from "../../components/ui";
import { useTheme, type MotionPref, type TextSize, type ThemePref } from "../../lib/theme";

/** A tiny window: sidebar lines and page lines, in the theme's colours. */
function ThemeTile({
  value,
  label,
  checked,
  onPick,
}: {
  value: ThemePref;
  label: string;
  checked: boolean;
  onPick: (v: ThemePref) => void;
}) {
  return (
    <button
      type="button"
      role="radio"
      aria-checked={checked}
      aria-label={label}
      className="k-theme-tile"
      data-preview={value}
      onClick={() => onPick(value)}
    >
      <span className="k-theme-tile__window" aria-hidden>
        <span className="k-theme-tile__side">
          <i style={{ width: "70%" }} />
          <i style={{ width: "50%" }} />
          <i style={{ width: "60%" }} />
        </span>
        <span className="k-theme-tile__main">
          <i className="k-theme-tile__island" />
          <i style={{ width: "60%" }} />
          <i style={{ width: "85%" }} />
          <i style={{ width: "40%" }} />
        </span>
      </span>
      <span className="k-theme-tile__label">
        <span className="k-radio" data-checked={checked || undefined} aria-hidden>
          {checked && <span className="k-radio__dot" />}
        </span>
        <b>{label}</b>
      </span>
    </button>
  );
}

export function AppearanceTab() {
  const { t } = useTranslation();
  const theme = useTheme();
  return (
    <>
      <Section title={t("settings.appearance.theme")} />
      <div className="k-theme-tiles" role="radiogroup" aria-label={t("settings.appearance.theme")}>
        {(["light", "dark", "system"] as const).map((v) => (
          <ThemeTile
            key={v}
            value={v}
            label={t(`settings.appearance.themes.${v}`)}
            checked={theme.theme === v}
            onPick={theme.setTheme}
          />
        ))}
      </div>
      <Note>{t("settings.appearance.systemNote")}</Note>

      <Section title={t("settings.appearance.accent")} />
      <Group>
        <Row
          icon="palette"
          title={t("settings.appearance.accent")}
          subtitle={t("settings.appearance.accentHint")}
          end={<AccentPicker value={theme.accent} onChange={theme.setAccent} />}
        />
      </Group>

      <Section title={t("settings.appearance.textMotion")} />
      <Group>
        <Row
          icon="type"
          title={t("settings.appearance.textSize")}
          end={
            <Segmented<TextSize>
              label={t("settings.appearance.textSize")}
              value={theme.textSize}
              onChange={theme.setTextSize}
              options={[
                { value: "normal", label: t("settings.appearance.sizes.normal") },
                { value: "large", label: t("settings.appearance.sizes.large") },
              ]}
            />
          }
        />
        <Row
          icon="motion"
          title={t("settings.appearance.motion")}
          subtitle={t("settings.appearance.motionHint")}
          end={
            <Segmented<MotionPref>
              label={t("settings.appearance.motion")}
              value={theme.motion}
              onChange={theme.setMotion}
              options={[
                { value: "system", label: t("settings.appearance.motions.system") },
                { value: "full", label: t("settings.appearance.motions.full") },
                { value: "reduced", label: t("settings.appearance.motions.reduced") },
              ]}
            />
          }
        />
        <Row
          icon="layers"
          title={t("settings.appearance.transparency")}
          subtitle={t("settings.appearance.transparencyHint")}
          end={
            <Switch
              label={t("settings.appearance.transparency")}
              checked={theme.transparency}
              onChange={theme.setTransparency}
            />
          }
        />
      </Group>
    </>
  );
}
