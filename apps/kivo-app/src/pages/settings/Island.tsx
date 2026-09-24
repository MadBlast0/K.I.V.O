/**
 * Settings → Island (UX-31, UX-13, UX-10, UX-15): its style, where and how big it is, what it
 * shows (the overlay style, UX-17) and how it behaves, with the wake glow (UX-16). The Character
 * style waits for its designed mascot and says so. With the Island hidden and every sound off, a
 * warning says KIVO would give no sign it's listening (UX-54).
 */
import { useTranslation } from "react-i18next";
import { Alert, Group, Orb, Pill, Row, Section, Segmented, Switch } from "../../components/ui";
import { bool, field, num, oneOf, setting } from "../../lib/settings";
import { useConfig } from "./useConfig";

type Style = "pill" | "orb" | "character" | "hidden";
const STYLES: ReadonlyArray<Style> = ["pill", "orb", "character", "hidden"];
/** Styles that exist today; the Character waits for its designed mascot (UX-38). */
const READY: ReadonlySet<Style> = new Set(["pill", "orb", "hidden"]);
type OverlayStyle = "pill-and-card" | "pill-only" | "card-only" | "off";
const OVERLAY_STYLES: ReadonlyArray<OverlayStyle> = ["pill-and-card", "pill-only", "card-only", "off"];

function StylePreview({ style }: { style: Style }) {
  if (style === "pill") {
    return (
      <span className="k-style-preview k-style-preview--pill" aria-hidden>
        <Orb size={11} />
        <i />
      </span>
    );
  }
  if (style === "orb") return <Orb size={30} />;
  if (style === "character") return <span className="k-style-preview k-style-preview--character" aria-hidden />;
  return <span className="k-style-preview k-style-preview--hidden" aria-hidden />;
}

export function IslandTab() {
  const { t } = useTranslation();
  const { settings, get, set } = useConfig();
  const style = oneOf(get("companion", "style"), STYLES) ?? "pill";
  const soundsOn = bool(get("sounds", "enabled"), true);
  const warn = bool(setting(settings, "accessibility", "warn-silent"), true);
  const live = get("automation", "live-activities");
  const liveOn = ["media", "timer", "download", "agent"].some((k) => bool(field(live, k), true));
  const hide = num(get("overlay", "hide-after-seconds"), 4);
  const hideValue = (["2", "4", "8", "0"] as const).find((v) => Number(v) === hide) ?? "4";

  return (
    <>
      {style === "hidden" && !soundsOn && warn && (
        <Alert kind="warning" title={t("settings.island.silentTitle")}>
          {t("settings.island.silentDetail")}
        </Alert>
      )}
      <Section title={t("settings.island.style")} />
      <div className="k-style-cards" role="radiogroup" aria-label={t("settings.island.style")}>
        {STYLES.map((s) => {
          const ready = READY.has(s);
          return (
            <button
              key={s}
              type="button"
              role="radio"
              aria-checked={style === s}
              aria-disabled={!ready}
              disabled={!ready}
              className="k-style-card"
              onClick={() => ready && set("companion", { style: s })}
            >
              <span className="k-style-card__preview">
                <StylePreview style={s} />
              </span>
              <b>{t(`settings.island.styles.${s}`)}</b>
              <span className="k-meta">{t(`settings.island.styles.${s}Hint`)}</span>
              {!ready && <Pill>{t("settings.general.later")}</Pill>}
            </button>
          );
        })}
      </div>

      <Section title={t("settings.island.placement")} />
      <Group>
        <Row
          icon="island"
          title={t("settings.island.position")}
          end={
            <Segmented<"top-center" | "bottom-center" | "remember-drag">
              label={t("settings.island.position")}
              value={
                oneOf(get("overlay", "position"), ["top-center", "bottom-center", "remember-drag"]) ?? "top-center"
              }
              onChange={(v) => set("overlay", { position: v })}
              options={(["top-center", "bottom-center", "remember-drag"] as const).map((value) => ({
                value,
                label: t(`settings.island.positions.${value}`),
              }))}
            />
          }
        />
        <Row
          icon="resize"
          title={t("settings.island.size")}
          end={
            <Segmented<"compact" | "standard" | "large">
              label={t("settings.island.size")}
              value={oneOf(get("overlay", "size"), ["compact", "standard", "large"]) ?? "standard"}
              onChange={(v) => set("overlay", { size: v })}
              options={(["compact", "standard", "large"] as const).map((value) => ({
                value,
                label: t(`settings.island.sizes.${value}`),
              }))}
            />
          }
        />
        <Row
          icon="monitor"
          title={t("settings.island.monitor")}
          subtitle={t("settings.island.monitorHint")}
          end={
            <Segmented<"active" | "main">
              label={t("settings.island.monitor")}
              value={oneOf(get("overlay", "monitor"), ["active", "main"]) ?? "active"}
              onChange={(v) => set("overlay", { monitor: v })}
              options={[
                { value: "active", label: t("settings.island.monitors.active") },
                { value: "main", label: t("settings.island.monitors.main") },
              ]}
            />
          }
        />
      </Group>

      <Section title={t("settings.island.shows")} />
      <Group>
        <Row
          icon="island"
          title={t("settings.island.overlayStyle")}
          subtitle={t(
            `settings.island.overlayStyles.${oneOf(get("overlay", "style"), OVERLAY_STYLES) ?? "pill-and-card"}Hint`,
          )}
          end={
            <Segmented<OverlayStyle>
              label={t("settings.island.overlayStyle")}
              value={oneOf(get("overlay", "style"), OVERLAY_STYLES) ?? "pill-and-card"}
              onChange={(v) => set("overlay", { style: v })}
              options={OVERLAY_STYLES.map((v) => ({ value: v, label: t(`settings.island.overlayStyles.${v}`) }))}
            />
          }
        />
        <Row
          icon="chat"
          title={t("settings.island.transcript")}
          end={
            <Switch
              label={t("settings.island.transcript")}
              checked={bool(get("overlay", "show-transcript"), true)}
              onChange={(v) => set("overlay", { "show-transcript": v })}
            />
          }
        />
        <Row
          icon="island"
          title={t("settings.island.live")}
          subtitle={t("settings.island.liveHint")}
          end={
            <Switch
              label={t("settings.island.live")}
              checked={liveOn}
              onChange={(v) => set("automation", { "live-activities": { media: v, timer: v, download: v, agent: v } })}
            />
          }
        />
        <Row
          icon="undo"
          title={t("settings.island.undo")}
          subtitle={t("settings.island.undoHint")}
          end={
            <Switch
              label={t("settings.island.undo")}
              checked={bool(get("overlay", "show-undo"), true)}
              onChange={(v) => set("overlay", { "show-undo": v })}
            />
          }
        />
        <Row
          icon="mic"
          title={t("settings.island.hints")}
          subtitle={t("settings.island.hintsHint")}
          end={
            <Switch
              label={t("settings.island.hints")}
              checked={bool(get("overlay", "voice-hints"), true)}
              onChange={(v) => set("overlay", { "voice-hints": v })}
            />
          }
        />
      </Group>

      <Section title={t("settings.island.behavior")} />
      <Group>
        <Row
          icon="clock"
          title={t("settings.island.hideAfter")}
          end={
            <Segmented<"2" | "4" | "8" | "0">
              label={t("settings.island.hideAfter")}
              value={hideValue}
              onChange={(v) => set("overlay", { "hide-after-seconds": Number(v) })}
              options={[
                { value: "2", label: t("settings.island.seconds", { count: 2 }) },
                { value: "4", label: t("settings.island.seconds", { count: 4 }) },
                { value: "8", label: t("settings.island.seconds", { count: 8 }) },
                { value: "0", label: t("settings.island.never") },
              ]}
            />
          }
        />
        <Row
          icon="glow"
          title={t("settings.island.glow")}
          subtitle={t("settings.island.glowHint")}
          end={
            <Switch
              label={t("settings.island.glow")}
              checked={bool(get("overlay", "wake-glow"))}
              onChange={(v) => set("overlay", { "wake-glow": v })}
            />
          }
        />
        <Row
          icon="window"
          title={t("settings.island.fullscreen")}
          end={
            <Segmented<"hide" | "tiny-pill">
              label={t("settings.island.fullscreen")}
              value={oneOf(get("overlay", "in-fullscreen"), ["hide", "tiny-pill"]) ?? "hide"}
              onChange={(v) => set("overlay", { "in-fullscreen": v })}
              options={[
                { value: "hide", label: t("settings.island.fullscreens.hide") },
                { value: "tiny-pill", label: t("settings.island.fullscreens.tinyPill") },
              ]}
            />
          }
        />
      </Group>
    </>
  );
}
