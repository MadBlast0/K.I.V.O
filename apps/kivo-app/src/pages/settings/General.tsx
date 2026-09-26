/**
 * Settings → General (UX-31): startup, language, people and the settings file (export, import,
 * reset). Everything is the runtime's `kivo.toml`.
 */
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Button,
  Checkbox,
  Dialog,
  Group,
  Meta,
  Pill,
  Row,
  Section,
  Select,
  Switch,
  useToast,
} from "../../components/ui";
import { LANGUAGES, applyLanguage } from "../../i18n";
import { Method } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { MODES } from "../../lib/modes";
import { bool, oneOf, str, strings } from "../../lib/settings";
import type { ProductMode } from "../../ipc/generated";
import { message, useConfig } from "./useConfig";

/** Languages people can speak to KIVO in; the speech engines' own lists decide what works. */
const SPOKEN = ["en", "hi", "pa", "es", "fr", "de", "it", "pt", "ja", "ko", "zh", "ar", "ru", "bn", "ta"] as const;

interface ImportReport {
  settings: boolean;
  routines: number;
  skipped: string[];
  notes: number;
}

export function GeneralTab() {
  const { t, i18n } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const { get, set } = useConfig();
  const [languages, setLanguages] = useState<string[] | null>(null);
  const [reset, setReset] = useState(false);
  const file = useRef<HTMLInputElement>(null);
  const names = new Intl.DisplayNames([i18n.language], { type: "language" });
  const spoken = strings(get("general", "languages"));
  const primary = str(get("general", "language"), "en");
  const all = [primary, ...spoken.filter((l) => l !== primary)];
  const mode =
    oneOf(
      get("general", "product-mode"),
      MODES.map((m) => m.mode),
    ) ?? "normal";
  const switchMode = (next: ProductMode) =>
    void request(Method.modeSet, { mode: next })
      .then(() => toast(t("modes.switched", { mode: t(`modes.${next}`) })))
      .catch((e: unknown) => toast(message(e)));

  return (
    <>
      <Section title={t("modes.title")} />
      <Group>
        <Row
          icon={MODES.find((m) => m.mode === mode)?.icon ?? "home"}
          title={t("modes.label")}
          subtitle={t(`modes.${mode}Hint`)}
          end={
            <Select<ProductMode>
              label={t("modes.label")}
              value={mode}
              onChange={switchMode}
              items={MODES.map((m) => ({ value: m.mode, label: t(`modes.${m.mode}`) }))}
            />
          }
        />
      </Group>

      <Section title={t("settings.general.startup")} />
      <Group>
        <Row
          icon="power"
          title={t("settings.general.startWithWindows")}
          subtitle={t("settings.general.startWithWindowsHint")}
          end={
            <Switch
              label={t("settings.general.startWithWindows")}
              checked={bool(get("general", "start-with-windows"))}
              onChange={(v) => set("general", { "start-with-windows": v })}
            />
          }
        />
        <Row
          icon="tray"
          title={t("settings.general.keepRunning")}
          subtitle={t("settings.general.keepRunningHint")}
          end={
            <Switch
              label={t("settings.general.keepRunning")}
              checked={bool(get("general", "keep-running-on-close"), true)}
              onChange={(v) => set("general", { "keep-running-on-close": v })}
            />
          }
        />
        <Row
          icon="tray"
          title={t("settings.general.trayIcon")}
          subtitle={t("settings.general.trayIconHint")}
          end={
            <Switch
              label={t("settings.general.trayIcon")}
              checked={bool(get("general", "tray-icon"), true)}
              onChange={(v) => set("general", { "tray-icon": v })}
            />
          }
        />
      </Group>

      <Group>
        <Row
          icon="cpu"
          title={t("settings.general.lowMemory")}
          subtitle={t("settings.general.lowMemoryHint")}
          end={
            <Switch
              label={t("settings.general.lowMemory")}
              checked={bool(get("general", "low-memory-mode"))}
              onChange={(v) => set("general", { "low-memory-mode": v })}
            />
          }
        />
      </Group>
      <Section title={t("settings.general.language")} />
      <Group>
        <Row
          icon="globe"
          title={t("settings.general.appLanguage")}
          subtitle={t("settings.general.appLanguageHint")}
          end={
            <Select
              label={t("settings.general.appLanguage")}
              value={i18n.language}
              onChange={applyLanguage}
              items={LANGUAGES.map((l) => ({ value: l.code, label: names.of(l.code) ?? l.name }))}
            />
          }
        />
        <Row
          icon="mic"
          title={t("settings.general.spoken")}
          subtitle={t("settings.general.spokenHint")}
          end={<Meta>{all.map((l) => names.of(l) ?? l).join(", ")}</Meta>}
          onClick={() => setLanguages(all)}
        />
        <Row
          icon="clock"
          title={t("settings.general.format")}
          subtitle={t("settings.general.formatExample", {
            example: new Intl.DateTimeFormat(
              oneOf(get("general", "format"), ["windows", "app"]) === "app" ? i18n.language : undefined,
              { dateStyle: "medium", timeStyle: "short" },
            ).format(new Date()),
          })}
          end={
            <Select<"windows" | "app">
              label={t("settings.general.format")}
              value={oneOf(get("general", "format"), ["windows", "app"]) ?? "windows"}
              onChange={(v) => set("general", { format: v })}
              items={[
                { value: "windows", label: t("settings.general.formatWindows") },
                { value: "app", label: t("settings.general.formatApp") },
              ]}
            />
          }
        />
      </Group>

      <Section title={t("settings.general.people")} />
      <Group>
        <Row
          icon="users"
          title={t("settings.general.profiles")}
          subtitle={t("settings.general.profilesHint")}
          end={<Pill>{t("settings.general.later")}</Pill>}
        />
      </Group>

      <Section title={t("settings.general.yourSettings")} />
      <Group>
        <Row
          icon="upload"
          title={t("settings.general.export")}
          subtitle={t("settings.general.exportHint")}
          end={
            <Button
              size="sm"
              onClick={() =>
                void request<{ file: string }>(Method.settingsExport)
                  .then((r) => toast(t("settings.general.exported", { file: r.file })))
                  .catch((e: unknown) => toast(message(e)))
              }
            >
              {t("settings.general.exportButton")}
            </Button>
          }
        />
        <Row
          icon="download"
          title={t("settings.general.import")}
          subtitle={t("settings.general.importHint")}
          end={
            <>
              <Button size="sm" onClick={() => file.current?.click()}>
                {t("settings.general.importButton")}
              </Button>
              <input
                ref={file}
                type="file"
                accept=".json,application/json"
                hidden
                aria-label={t("settings.general.import")}
                onChange={(e) => {
                  const chosen = e.target.files?.[0];
                  e.target.value = "";
                  if (!chosen) return;
                  void chosen
                    .text()
                    .then((content) => request<ImportReport>(Method.settingsImport, { content }))
                    .then((r) =>
                      toast(
                        t("settings.general.imported", {
                          routines: r.routines,
                          notes: r.notes,
                          skipped: r.skipped.length,
                        }),
                      ),
                    )
                    .catch((err: unknown) => toast(message(err)));
                }}
              />
            </>
          }
        />
        <Row
          icon="refresh"
          title={t("settings.general.reset")}
          subtitle={t("settings.general.resetHint")}
          end={
            <Button size="sm" variant="destructive" onClick={() => setReset(true)}>
              {t("settings.general.resetButton")}
            </Button>
          }
        />
      </Group>

      <Dialog
        open={languages !== null}
        onOpenChange={(o) => !o && setLanguages(null)}
        title={t("settings.general.spoken")}
        description={t("settings.general.spokenDetail")}
        footer={
          <>
            <Button variant="plain" onClick={() => setLanguages(null)}>
              {t("ui.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={!languages || languages.length === 0}
              onClick={() => {
                if (languages && languages.length > 0) {
                  const [first, ...rest] = languages;
                  set("general", { language: first, languages: rest });
                }
                setLanguages(null);
              }}
            >
              {t("ui.save")}
            </Button>
          </>
        }
      >
        <div className="k-checklist">
          {SPOKEN.map((code) => (
            <Checkbox
              key={code}
              checked={languages?.includes(code) ?? false}
              onChange={(on) =>
                setLanguages((l) => {
                  const list = l ?? [];
                  return on ? [...list, code] : list.filter((x) => x !== code);
                })
              }
            >
              {names.of(code) ?? code}
              {languages?.[0] === code && <Meta> · {t("settings.general.primary")}</Meta>}
            </Checkbox>
          ))}
        </div>
      </Dialog>

      <Dialog
        open={reset}
        onOpenChange={setReset}
        title={t("settings.general.resetTitle")}
        description={t("settings.general.resetConfirm")}
        footer={
          <>
            <Button variant="plain" onClick={() => setReset(false)}>
              {t("ui.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                setReset(false);
                void request(Method.settingsReset)
                  .then(() => toast(t("settings.general.resetDone")))
                  .catch((e: unknown) => toast(message(e)));
              }}
            >
              {t("settings.general.resetButton")}
            </Button>
          </>
        }
      />
    </>
  );
}
