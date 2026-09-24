/**
 * Permissions (UX-28): Mode · Capabilities · Privacy.
 *
 * - Mode (SECURITY §1.1): how much KIVO does before asking, the hard limits that hold in every
 *   mode, a comparison of the modes, and the permissions the user has granted, with how long
 *   they last and a Revoke button (SEC-08).
 * - Capabilities (CAP-04/05/07): presets, every capability with its toggle, badges and when it
 *   was last used, and each capability's own options (allowed and private folders, per-app and
 *   per-site lists, cloud vision, clipboard read/write, read-only shell).
 * - Privacy (SECURITY §6): where requests may go, how long conversations are kept, private
 *   folders and transcript logging.
 * Every change goes to the runtime, which owns the settings.
 */
import { useCallback, useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import { ComputerUseOptions } from "../components/permissions/ComputerUseOptions";
import {
  Alert,
  Button,
  Checkbox,
  Dialog,
  Group,
  IconButton,
  Keys,
  Note,
  OptionCard,
  PageTabs,
  Pill,
  RadioGroup,
  Row,
  Section,
  Segmented,
  Select,
  Sheet,
  Switch,
  Tag,
  TextField,
  useToast,
} from "../components/ui";
import type { IconName } from "../icons";
import {
  Method,
  type Badge,
  type Capability,
  type CapabilityItem,
  type GrantItem,
  type PermissionMode,
  type Preset,
} from "../ipc/generated";
import { useRuntime } from "../ipc/runtime";
import { ago } from "../lib/ago";
import { bool, num, str } from "../lib/settings";

export const PERMISSIONS_TABS = ["mode", "capabilities", "privacy"] as const;
type Tab = (typeof PERMISSIONS_TABS)[number];

export function Permissions({ initialTab = "mode" }: { initialTab?: Tab }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>(initialTab);
  return (
    <>
      <PageHeader title={t("nav.permissions")} subtitle={t("permissions.subtitle")} />
      <PageTabs<Tab>
        label={t("nav.permissions")}
        value={tab}
        onChange={setTab}
        tabs={[
          { value: "mode", label: t("permissions.tabs.mode"), content: <ModeTab /> },
          { value: "capabilities", label: t("permissions.tabs.capabilities"), content: <CapabilitiesTab /> },
          { value: "privacy", label: t("permissions.tabs.privacy"), content: <PrivacyTab /> },
        ]}
      />
    </>
  );
}

function useFail() {
  const toast = useToast();
  return useCallback((e: unknown) => toast(e instanceof Error ? e.message : String(e)), [toast]);
}

/** The settings as the runtime has them (kebab-case keys), reloaded after every change. */
type Settings = Record<string, Record<string, unknown>>;

function useSettings(): [Settings | null, (patch: Settings) => void] {
  const { link, request } = useRuntime();
  const fail = useFail();
  const connected = link?.status === "connected";
  const [settings, setSettings] = useState<Settings | null>(null);
  useEffect(() => {
    if (!connected) return;
    void request<Settings>(Method.settingsGet)
      .then(setSettings)
      .catch(() => {});
  }, [connected, request]);
  const save = useCallback(
    (patch: Settings) => {
      request<Settings>(Method.settingsSet, patch).then(setSettings).catch(fail);
    },
    [request, fail],
  );
  return [settings, save];
}

function strings(v: unknown): string[] {
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
}

/** A value from the settings when it is one of `allowed`. */
function oneOf<T extends string>(v: unknown, allowed: ReadonlyArray<T>): T | undefined {
  return allowed.find((a) => a === v);
}

function field(v: unknown, key: string): unknown {
  if (typeof v !== "object" || v === null) return undefined;
  return Object.entries(v).find(([k]) => k === key)?.[1];
}

/* ───────────────────────────── Mode ───────────────────────────── */

const MODES: ReadonlyArray<PermissionMode> = ["ask", "accept-edits", "plan", "auto"];

function ModeTab() {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const fail = useFail();
  const connected = link?.status === "connected";
  const mode = link?.status === "connected" ? link.snapshot?.mode : undefined;
  const [grants, setGrants] = useState<GrantItem[]>([]);

  const loadGrants = useCallback(() => {
    if (!connected) return;
    void request<GrantItem[]>(Method.permissionsGrants)
      .then(setGrants)
      .catch(() => {});
  }, [connected, request]);
  useEffect(loadGrants, [loadGrants]);

  const revoke = (id: number) => {
    request(Method.permissionsRevoke, { id }).then(loadGrants).catch(fail);
  };

  if (!connected) return <Note>{t("voice.notConnected")}</Note>;
  const date = new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" });
  const lasts = (g: GrantItem) =>
    g.sessionOnly
      ? t("permissions.grants.session")
      : g.expiresAt
        ? t("permissions.grants.until", { when: date.format(new Date(g.expiresAt)) })
        : t("permissions.grants.always");

  return (
    <>
      <RadioGroup<PermissionMode>
        label={t("permissions.mode.title")}
        value={mode === "bypass" ? undefined : mode}
        onChange={(next) => {
          request(Method.permissionsSetMode, { mode: next }).catch(fail);
        }}
      >
        {MODES.map((m) => (
          <OptionCard
            key={m}
            value={m}
            title={t(`mode.${m}`)}
            description={t(`permissions.mode.${m}`)}
            badge={m === "auto" ? <Pill tone="accent">{t("permissions.recommended")}</Pill> : undefined}
          />
        ))}
      </RadioGroup>
      <Note>
        {t("permissions.mode.switch")} <Keys keys={["Ctrl", "Shift", "M"]} />
      </Note>
      <Bypass until={link?.status === "connected" ? link.snapshot?.bypassUntil : undefined} />

      <Section title={t("permissions.limits.title")} />
      <Group>
        <Row icon="stop" title={t("permissions.limits.stop")} end={<Keys keys={["Ctrl", "Alt", "Shift", "Esc"]} />} />
        <Row icon="capabilities" title={t("permissions.limits.capabilities")} />
        <Row icon="privacy" title={t("permissions.limits.passwords")} />
        <Row icon="permissions" title={t("permissions.limits.apps")} subtitle={t("permissions.limits.appsHint")} />
        <Row
          icon="send"
          title={t("permissions.limits.destinations")}
          subtitle={t("permissions.limits.destinationsHint")}
        />
      </Group>

      <Section title={t("permissions.compare.title")} />
      <div className="k-compare" role="table" aria-label={t("permissions.compare.title")}>
        <div role="row" className="k-compare__head">
          <span role="columnheader">{t("permissions.compare.action")}</span>
          {MODES.map((m) => (
            <span role="columnheader" key={m}>
              {t(`permissions.compare.short.${m}`)}
            </span>
          ))}
        </div>
        {(["read", "apps", "files", "commands", "high"] as const).map((row) => (
          <div role="row" key={row} className="k-compare__row">
            <span role="cell">{t(`permissions.compare.rows.${row}`)}</span>
            {MODES.map((m) => {
              const v = compare(row, m);
              return (
                <span role="cell" key={m} className={`k-compare__cell k-compare__cell--${v}`}>
                  {t(`permissions.compare.values.${v}`)}
                </span>
              );
            })}
          </div>
        ))}
      </div>

      <Section title={t("permissions.grants.title")} aside={t("permissions.grants.hint")} />
      {grants.length === 0 ? (
        <Note>{t("permissions.grants.none")}</Note>
      ) : (
        <Group>
          {grants.map((g) => (
            <Row
              key={g.id}
              icon="check"
              title={g.tool}
              subtitle={[g.scope, g.pattern, lasts(g)].filter(Boolean).join(" · ")}
              end={
                <Button size="sm" onClick={() => revoke(g.id)}>
                  {t("permissions.grants.revoke")}
                </Button>
              }
            />
          ))}
        </Group>
      )}
    </>
  );
}

/** How long Bypass lasts (SEC-03): 15 minutes, an hour (the default) or until turned off. */
type BypassFor = "15" | "60" | "off";

/**
 * Bypass permissions (SEC-03): KIVO acts without asking, High risk included, until it expires.
 * Only from this dialog (never the tray or a menu click), optionally confirmed with Windows Hello;
 * hard limits still hold, every action is audited, and guests can't use it.
 */
function Bypass({ until }: { until: number | null | undefined }) {
  const { t, i18n } = useTranslation();
  const { request } = useRuntime();
  const fail = useFail();
  const [open, setOpen] = useState(false);
  const [lasts, setLasts] = useState<BypassFor>("60");
  const [hello, setHello] = useState(true);
  const on = until != null;
  const time = new Intl.DateTimeFormat(i18n.language, { timeStyle: "short" });
  // The runtime sends u64::MAX for "until turned off".
  const forever = on && until >= Number.MAX_SAFE_INTEGER;
  const turnOn = () => {
    request(Method.permissionsBypass, {
      on: true,
      ...(lasts === "off" ? {} : { minutes: Number(lasts) }),
      hello,
    })
      .then(() => setOpen(false))
      .catch(fail);
  };
  return (
    <>
      <Section title={t("permissions.bypass.title")} />
      <Group>
        <Row
          icon="warning"
          title={on ? t("permissions.bypass.on") : t("permissions.bypass.off")}
          subtitle={
            on
              ? forever
                ? t("permissions.bypass.untilOff")
                : t("permissions.bypass.until", { time: time.format(new Date(until)) })
              : t("permissions.bypass.detail")
          }
          end={
            on ? (
              <Button
                variant="stop"
                onClick={() => {
                  request(Method.permissionsBypass, { on: false }).catch(fail);
                }}
              >
                {t("permissions.bypass.turnOff")}
              </Button>
            ) : (
              <Button variant="destructive" onClick={() => setOpen(true)}>
                {t("permissions.bypass.turnOn")}
              </Button>
            )
          }
        />
      </Group>
      <Dialog
        open={open}
        onOpenChange={setOpen}
        title={t("permissions.bypass.dialogTitle")}
        description={t("permissions.bypass.dialogDetail")}
        footer={
          <>
            <Button onClick={() => setOpen(false)}>{t("permissions.bypass.cancel")}</Button>
            <Button variant="stop" onClick={turnOn}>
              {t("permissions.bypass.turnOn")}
            </Button>
          </>
        }
      >
        <Alert kind="danger" title={t("permissions.bypass.warnTitle")}>
          {t("permissions.bypass.warn")}
        </Alert>
        <Section title={t("permissions.bypass.expires")} />
        <Segmented<BypassFor>
          label={t("permissions.bypass.expires")}
          value={lasts}
          onChange={setLasts}
          options={[
            { value: "15", label: t("permissions.bypass.for15") },
            { value: "60", label: t("permissions.bypass.for60") },
            { value: "off", label: t("permissions.bypass.forOff") },
          ]}
        />
        <div className="k-dialog__row">
          <Checkbox checked={hello} onChange={setHello}>
            {t("permissions.bypass.hello")}
          </Checkbox>
        </div>
      </Dialog>
    </>
  );
}

/** The mode table (SECURITY §1.1): what each mode does for each kind of action. */
function compare(
  row: "read" | "apps" | "files" | "commands" | "high",
  mode: PermissionMode,
): "none" | "ask" | "auto" | "plan" {
  if (row === "read") return "none";
  if (mode === "plan") return "plan";
  if (mode === "ask") return "ask";
  if (row === "high") return "ask";
  if (mode === "accept-edits") return row === "commands" ? "ask" : "auto";
  return "auto";
}

/* ─────────────────────────── Capabilities ─────────────────────────── */

const SECTIONS: ReadonlyArray<{ id: string; items: ReadonlyArray<Capability> }> = [
  { id: "voice", items: ["mic-listening", "push-to-talk", "speak-responses", "realtime-voice", "speaker-recognition"] },
  {
    id: "computer",
    items: ["apps-and-windows", "system-controls", "power-actions", "files-read", "files-modify", "clipboard", "shell"],
  },
  { id: "screen", items: ["ui-automation", "screen-awareness", "computer-use"] },
  { id: "browser", items: ["browser-open-links", "browser-pages", "browser-autonomous"] },
  {
    id: "intelligence",
    items: [
      "cloud-brains",
      "cli-agents",
      "mcp-servers",
      "integrations",
      "memory",
      "routines",
      "background-tasks",
      "notifications",
      "remote-access",
    ],
  },
];

const ICON: Record<Capability, IconName> = {
  "mic-listening": "mic",
  "push-to-talk": "keyboard",
  "speak-responses": "wave",
  "apps-and-windows": "window",
  "system-controls": "volume",
  "power-actions": "power",
  "files-read": "folder",
  "files-modify": "file",
  clipboard: "clipboard",
  "browser-open-links": "link",
  "browser-pages": "globe",
  "browser-autonomous": "compass",
  "ui-automation": "cursor",
  "screen-awareness": "eye",
  "computer-use": "hand",
  shell: "terminal",
  "background-tasks": "tasks",
  routines: "routine",
  memory: "memory",
  "cloud-brains": "cloud",
  "realtime-voice": "headset",
  "cli-agents": "code",
  "mcp-servers": "server",
  integrations: "connector",
  "speaker-recognition": "user",
  "remote-access": "phone",
  notifications: "bell",
};

/** Capabilities with their own options (CAPABILITIES §1 "Controls when on"). */
const WITH_OPTIONS: ReadonlySet<Capability> = new Set<Capability>([
  "apps-and-windows",
  "files-read",
  "files-modify",
  "clipboard",
  "shell",
  "ui-automation",
  "screen-awareness",
  "computer-use",
  "browser-pages",
  "realtime-voice",
]);

const BADGE_TONE: Record<Badge, "success" | "accent" | "warning" | "neutral"> = {
  local: "success",
  cloud: "accent",
  costly: "warning",
  sensitive: "neutral",
};

function useAgo() {
  const { t, i18n } = useTranslation();
  return (ms: number | null) => {
    if (ms === null) return t("permissions.capabilities.neverUsed");
    return t("permissions.capabilities.used", { when: ago(ms, i18n.language) });
  };
}

function CapabilitiesTab() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const fail = useFail();
  const usedAgo = useAgo();
  const connected = link?.status === "connected";
  const [items, setItems] = useState<CapabilityItem[] | null>(null);
  const [settings, save] = useSettings();
  const [options, setOptions] = useState<Capability | null>(null);

  useEffect(() => {
    if (!connected) return;
    void request<CapabilityItem[]>(Method.capabilitiesGet)
      .then(setItems)
      .catch(() => {});
  }, [connected, request]);

  if (!connected) return <Note>{t("voice.notConnected")}</Note>;
  if (!items) return null;
  const byId = new Map(items.map((i) => [i.capability, i]));
  const preset: Preset =
    oneOf(settings?.tools?.["preset"], ["minimal", "balanced", "power-user", "custom"] as const) ?? "balanced";
  const toggle = (capability: Capability, on: boolean) => {
    request<CapabilityItem[]>(Method.capabilitiesSet, { capability, on })
      .then((next) => {
        setItems(next);
        save({});
      })
      .catch(fail);
  };
  const applyPreset = (next: Preset) => {
    request<CapabilityItem[]>(Method.capabilitiesPreset, { preset: next })
      .then((list) => {
        setItems(list);
        save({});
      })
      .catch(fail);
  };

  return (
    <>
      <Group>
        <Row
          icon="capabilities"
          title={t("permissions.capabilities.preset")}
          subtitle={t(`permissions.capabilities.presetHint.${preset}`)}
          end={
            <Segmented<Preset>
              label={t("permissions.capabilities.preset")}
              value={preset === "custom" ? undefined : preset}
              onChange={applyPreset}
              options={(["minimal", "balanced", "power-user"] as const).map((p) => ({
                value: p,
                label: t(`permissions.capabilities.presets.${p}`),
              }))}
            />
          }
        />
      </Group>
      {SECTIONS.map((section) => (
        <div key={section.id}>
          <Section title={t(`permissions.capabilities.sections.${section.id}`)} />
          <Group>
            {section.items.map((c) => {
              const item = byId.get(c);
              if (!item) return null;
              return (
                <Row
                  key={c}
                  icon={ICON[c]}
                  title={item.label}
                  subtitle={`${t(`permissions.capabilities.about.${c}`)} · ${usedAgo(item.lastUsed)}`}
                  end={
                    <span className="k-inline">
                      {item.badges.map((b) => (
                        <Tag key={b} tone={BADGE_TONE[b]}>
                          {t(`permissions.badges.${b}`)}
                        </Tag>
                      ))}
                      {WITH_OPTIONS.has(c) && (
                        <Button size="sm" variant="plain" onClick={() => setOptions(c)}>
                          {t("permissions.capabilities.options")}
                        </Button>
                      )}
                      <Switch label={item.label} checked={item.enabled} onChange={(on) => toggle(c, on)} />
                    </span>
                  }
                />
              );
            })}
          </Group>
        </div>
      ))}
      <Note>{t("permissions.capabilities.footer")}</Note>
      <Sheet
        open={options !== null}
        onOpenChange={(open) => !open && setOptions(null)}
        title={options ? (byId.get(options)?.label ?? "") : ""}
      >
        {options && settings && <CapabilityOptions capability={options} settings={settings} save={save} />}
      </Sheet>
    </>
  );
}

/** A list of names or folders the user edits: add, remove. */
function ListEditor({
  items,
  onChange,
  placeholder,
  label,
}: {
  items: string[];
  onChange: (items: string[]) => void;
  placeholder: string;
  label: string;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState("");
  const add = () => {
    const value = draft.trim();
    if (!value || items.includes(value)) return;
    onChange([...items, value]);
    setDraft("");
  };
  return (
    <>
      {items.length > 0 && (
        <Group>
          {items.map((item) => (
            <Row
              key={item}
              title={item}
              end={
                <IconButton
                  icon="delete"
                  size="sm"
                  label={t("permissions.remove", { item })}
                  onClick={() => onChange(items.filter((i) => i !== item))}
                />
              }
            />
          ))}
        </Group>
      )}
      <form
        className="k-inline"
        onSubmit={(e) => {
          e.preventDefault();
          add();
        }}
      >
        <TextField
          aria-label={label}
          placeholder={placeholder}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
        />
        <Button type="submit" size="sm" icon="add" disabled={!draft.trim()}>
          {t("permissions.add")}
        </Button>
      </form>
    </>
  );
}

/** Realtime voice's options (BRAINS §8, BRAIN-33): which service, and when a quiet session ends. */
function RealtimeOptions({ settings, save }: { settings: Settings; save: (p: Settings) => void }) {
  const { t } = useTranslation();
  const realtime = field(settings.brains, "realtime");
  const set = (key: string, value: unknown) => save({ brains: { realtime: { [key]: value } } });
  const provider = oneOf(field(realtime, "provider"), ["", "openai", "gemini"] as const) ?? "";
  const silence = String(num(field(realtime, "silence-seconds"), 15));
  return (
    <Group>
      <Row
        title={t("permissions.rt.provider")}
        subtitle={t("permissions.rt.providerHint")}
        end={
          <Select<string>
            label={t("permissions.rt.provider")}
            value={provider || "auto"}
            onChange={(v) => set("provider", v === "auto" ? "" : v)}
            items={[
              { value: "auto", label: t("permissions.rt.auto") },
              { value: "openai", label: "OpenAI Realtime" },
              { value: "gemini", label: "Gemini Live" },
            ]}
          />
        }
      />
      <Row
        title={t("permissions.rt.silence")}
        subtitle={t("permissions.rt.silenceHint")}
        end={
          <Select<string>
            label={t("permissions.rt.silence")}
            value={silence}
            onChange={(v) => set("silence-seconds", Number(v))}
            items={["10", "15", "30", "60"].map((s) => ({
              value: s,
              label: t("permissions.rt.seconds", { count: Number(s) }),
            }))}
          />
        }
      />
    </Group>
  );
}

function AppLists({ scope, settings, save }: { scope: string; settings: Settings; save: (p: Settings) => void }) {
  const { t } = useTranslation();
  const lists = settings.tools?.[scope];
  const allow = strings(field(lists, "allow"));
  const block = strings(field(lists, "block"));
  const set = (key: "allow" | "block", value: string[]) => save({ tools: { [scope]: { allow, block, [key]: value } } });
  return (
    <>
      <Section title={t("permissions.options.block")} aside={t("permissions.options.blockHint")} />
      <ListEditor
        label={t("permissions.options.block")}
        items={block}
        onChange={(v) => set("block", v)}
        placeholder={t("permissions.options.appPlaceholder")}
      />
      <Section title={t("permissions.options.allow")} aside={t("permissions.options.allowHint")} />
      <ListEditor
        label={t("permissions.options.allow")}
        items={allow}
        onChange={(v) => set("allow", v)}
        placeholder={t("permissions.options.appPlaceholder")}
      />
    </>
  );
}

function BrowserStatus() {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const [status, setStatus] = useState<{ connected: boolean; extensionId: string; folder: string | null } | null>(null);
  useEffect(() => {
    void request<{ connected: boolean; extensionId: string; folder: string | null }>(Method.browserStatus)
      .then(setStatus)
      .catch(() => {});
  }, [request]);
  if (!status) return null;
  return (
    <Group>
      <Row
        icon="globe"
        title={t("permissions.options.extension")}
        subtitle={status.connected ? t("permissions.options.extensionOn") : t("permissions.options.extensionOff")}
        end={
          <Tag tone={status.connected ? "success" : "neutral"}>
            {status.connected ? t("permissions.options.connected") : t("permissions.options.notConnected")}
          </Tag>
        }
      />
      {!status.connected && status.folder && (
        <Row
          title={t("permissions.options.install")}
          subtitle={t("permissions.options.installHow", { folder: status.folder })}
        />
      )}
    </Group>
  );
}

function CapabilityOptions({
  capability,
  settings,
  save,
}: {
  capability: Capability;
  settings: Settings;
  save: (patch: Settings) => void;
}) {
  const { t } = useTranslation();
  const tools = settings.tools ?? {};
  const option = (key: string, title: string, hint?: string): ReactNode => (
    <Row
      title={title}
      subtitle={hint}
      end={<Switch label={title} checked={tools[key] === true} onChange={(v) => save({ tools: { [key]: v } })} />}
    />
  );
  switch (capability) {
    case "apps-and-windows":
      return (
        <>
          <Note>{t("permissions.options.blockedAppsHint")}</Note>
          <ListEditor
            label={t("permissions.options.blockedApps")}
            items={strings(settings.permissions?.["blocked-apps"])}
            onChange={(v) => save({ permissions: { "blocked-apps": v } })}
            placeholder={t("permissions.options.appPlaceholder")}
          />
        </>
      );
    case "files-read":
    case "files-modify":
      return (
        <>
          <Section
            title={t("permissions.options.allowedFolders")}
            aside={t("permissions.options.allowedFoldersHint")}
          />
          <ListEditor
            label={t("permissions.options.allowedFolders")}
            items={strings(tools["allowed-folders"])}
            onChange={(v) => save({ tools: { "allowed-folders": v } })}
            placeholder={t("permissions.options.folderPlaceholder")}
          />
          <Section
            title={t("permissions.options.privateFolders")}
            aside={t("permissions.options.privateFoldersHint")}
          />
          <ListEditor
            label={t("permissions.options.privateFolders")}
            items={strings(tools["private-folders"])}
            onChange={(v) => save({ tools: { "private-folders": v } })}
            placeholder={t("permissions.options.folderPlaceholder")}
          />
        </>
      );
    case "clipboard":
      return (
        <Group>
          {option("clipboard-read", t("permissions.options.clipboardRead"), t("permissions.options.clipboardReadHint"))}
          {option("clipboard-write", t("permissions.options.clipboardWrite"))}
        </Group>
      );
    case "shell":
      return (
        <Group>
          {option(
            "shell-read-only",
            t("permissions.options.shellReadOnly"),
            t("permissions.options.shellReadOnlyHint"),
          )}
        </Group>
      );
    case "screen-awareness":
      return (
        <>
          <Group>
            {option("cloud-vision", t("permissions.options.cloudVision"), t("permissions.options.cloudVisionHint"))}
          </Group>
          <AppLists scope="screen-apps" settings={settings} save={save} />
        </>
      );
    case "ui-automation":
      return <AppLists scope="ui-automation-apps" settings={settings} save={save} />;
    case "computer-use":
      return (
        <ComputerUseOptions
          settings={settings}
          save={save}
          apps={<AppLists scope="computer-use-apps" settings={settings} save={save} />}
        />
      );
    case "realtime-voice":
      return <RealtimeOptions settings={settings} save={save} />;
    case "browser-pages":
      return (
        <>
          <BrowserStatus />
          <Section title={t("permissions.options.blockedSites")} />
          <ListEditor
            label={t("permissions.options.blockedSites")}
            items={strings(tools["blocked-sites"])}
            onChange={(v) => save({ tools: { "blocked-sites": v } })}
            placeholder={t("permissions.options.sitePlaceholder")}
          />
          <Section title={t("permissions.options.allowedSites")} aside={t("permissions.options.allowedSitesHint")} />
          <ListEditor
            label={t("permissions.options.allowedSites")}
            items={strings(tools["allowed-sites"])}
            onChange={(v) => save({ tools: { "allowed-sites": v } })}
            placeholder={t("permissions.options.sitePlaceholder")}
          />
        </>
      );
    default:
      return null;
  }
}

/* ───────────────────────────── Privacy ───────────────────────────── */

type PrivacyMode = "cloud" | "local" | "strict-private" | "custom";
const PRIVACY: ReadonlyArray<PrivacyMode> = ["cloud", "local", "strict-private", "custom"];
/** Custom mode's switches (SEC-21). */
const CUSTOM = ["cloud-brains", "personal-to-cloud", "cloud-speech", "cloud-vision"] as const;
const LABEL_CLASSES = ["personal", "sensitive", "highly-sensitive"] as const;
type LabelClass = (typeof LABEL_CLASSES)[number];
interface PrivacyLabel {
  text: string;
  class: LabelClass;
}

/** Words the user labels private (SEC-20): any text with one gets its class. */
function Labels({ labels, onChange }: { labels: PrivacyLabel[]; onChange: (l: PrivacyLabel[]) => void }) {
  const { t } = useTranslation();
  const [text, setText] = useState("");
  const [cls, setCls] = useState<LabelClass>("sensitive");
  return (
    <>
      {labels.length > 0 && (
        <Group>
          {labels.map((l) => (
            <Row
              key={l.text}
              icon="privacy"
              title={l.text}
              subtitle={t(`permissions.privacy.classes.${l.class}`)}
              end={
                <IconButton
                  icon="delete"
                  size="sm"
                  label={t("permissions.remove", { item: l.text })}
                  onClick={() => onChange(labels.filter((x) => x.text !== l.text))}
                />
              }
            />
          ))}
        </Group>
      )}
      <form
        className="k-inline"
        onSubmit={(e) => {
          e.preventDefault();
          const value = text.trim();
          if (!value || labels.some((l) => l.text.toLowerCase() === value.toLowerCase())) return;
          onChange([...labels, { text: value, class: cls }]);
          setText("");
        }}
      >
        <TextField
          value={text}
          onChange={(e) => setText(e.target.value)}
          placeholder={t("permissions.privacy.labelPlaceholder")}
          aria-label={t("permissions.privacy.labels")}
        />
        <Select
          label={t("permissions.privacy.labelClass")}
          value={cls}
          onChange={setCls}
          items={LABEL_CLASSES.map((c) => ({ value: c, label: t(`permissions.privacy.classes.${c}`) }))}
        />
        <Button type="submit" icon="add" disabled={!text.trim()}>
          {t("permissions.add")}
        </Button>
      </form>
    </>
  );
}
const KEEP: ReadonlyArray<{ days: number; key: string }> = [
  { days: 7, key: "week" },
  { days: 30, key: "month" },
  { days: 3650, key: "forever" },
  { days: 0, key: "never" },
];

/** The labels as saved, dropping anything malformed. */
function labelsOf(value: unknown): PrivacyLabel[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((v) => {
    const text = str(field(v, "text"));
    const cls = oneOf(field(v, "class"), LABEL_CLASSES);
    return text && cls ? [{ text, class: cls }] : [];
  });
}

function PrivacyTab() {
  const { t } = useTranslation();
  const { link } = useRuntime();
  const [settings, save] = useSettings();
  const [confirmNever, setConfirmNever] = useState(false);
  if (link?.status !== "connected") return <Note>{t("voice.notConnected")}</Note>;
  if (!settings) return null;
  const privacy = settings.privacy ?? {};
  const mode: PrivacyMode = oneOf(privacy["mode"], ["cloud", "local", "strict-private", "custom"] as const) ?? "cloud";
  const retention = privacy["retention-days"];
  const days = typeof retention === "number" ? retention : 30;
  const keep = KEEP.find((k) => k.days === days)?.key ?? "month";
  return (
    <>
      <Section title={t("permissions.privacy.where")} />
      <RadioGroup<PrivacyMode>
        label={t("permissions.privacy.where")}
        value={mode}
        onChange={(m) => save({ privacy: { mode: m } })}
      >
        {PRIVACY.map((m) => (
          <OptionCard
            key={m}
            value={m}
            title={t(`permissions.privacy.modes.${m}`)}
            description={t(`permissions.privacy.modes.${m}Hint`)}
            badge={m === "cloud" ? <Pill tone="accent">{t("permissions.recommended")}</Pill> : undefined}
          />
        ))}
      </RadioGroup>
      {mode === "custom" && (
        <>
          <Section title={t("permissions.privacy.customTitle")} />
          <Group>
            {CUSTOM.map((k) => (
              <Row
                key={k}
                title={t(`permissions.privacy.custom.${k}`)}
                subtitle={t(`permissions.privacy.custom.${k}Hint`)}
                end={
                  <Switch
                    label={t(`permissions.privacy.custom.${k}`)}
                    checked={bool(field(privacy["custom"], k), k === "cloud-brains")}
                    onChange={(v) => save({ privacy: { custom: { [k]: v } } })}
                  />
                }
              />
            ))}
          </Group>
        </>
      )}

      <Section title={t("permissions.privacy.stays")} />
      <Group>
        <Row
          icon="mic"
          title={t("permissions.privacy.wake")}
          subtitle={t("permissions.privacy.wakeHint")}
          end={<Tag tone="success">{t("permissions.privacy.onPc")}</Tag>}
        />
        <Row
          icon="user"
          title={t("permissions.privacy.voice")}
          end={<Tag tone="success">{t("permissions.privacy.onPc")}</Tag>}
        />
        <Row
          icon="key"
          title={t("permissions.privacy.secrets")}
          subtitle={t("permissions.privacy.secretsHint")}
          end={<Tag tone="success">{t("permissions.privacy.onPc")}</Tag>}
        />
        <Row
          icon="eye"
          title={t("permissions.privacy.screenshots")}
          subtitle={t("permissions.privacy.screenshotsHint")}
          end={<Tag tone="success">{t("permissions.privacy.onPc")}</Tag>}
        />
      </Group>

      <Section title={t("permissions.privacy.data")} />
      <Group>
        <Row
          icon="chat"
          title={t("permissions.privacy.keep")}
          end={
            <Select
              label={t("permissions.privacy.keep")}
              value={keep}
              onChange={(k) => {
                const choice = KEEP.find((x) => x.key === k);
                if (!choice) return;
                if (choice.days === 0) setConfirmNever(true);
                else save({ privacy: { "retention-days": choice.days } });
              }}
              items={KEEP.map((k) => ({ value: k.key, label: t(`permissions.privacy.keepFor.${k.key}`) }))}
            />
          }
        />
        <Row
          icon="file"
          title={t("permissions.privacy.transcripts")}
          subtitle={t("permissions.privacy.transcriptsHint")}
          end={
            <Switch
              label={t("permissions.privacy.transcripts")}
              checked={privacy["debug-transcripts"] === true}
              onChange={(v) => save({ privacy: { "debug-transcripts": v } })}
            />
          }
        />
        <Row
          icon="refresh"
          title={t("permissions.privacy.catalogs")}
          subtitle={t("permissions.privacy.catalogsHint")}
          end={
            <Switch
              label={t("permissions.privacy.catalogs")}
              checked={privacy["catalog-updates"] !== false}
              onChange={(v) => save({ privacy: { "catalog-updates": v } })}
            />
          }
        />
      </Group>
      <Section title={t("permissions.privacy.localFolders")} aside={t("permissions.privacy.localFoldersHint")} />
      <ListEditor
        label={t("permissions.privacy.localFolders")}
        items={strings(privacy["sensitive-folders"])}
        onChange={(v) => save({ privacy: { "sensitive-folders": v } })}
        placeholder={t("permissions.options.folderPlaceholder")}
      />
      <Section title={t("permissions.privacy.labels")} aside={t("permissions.privacy.labelsHint")} />
      <Labels labels={labelsOf(privacy["labels"])} onChange={(labels) => save({ privacy: { labels } })} />
      <Section title={t("permissions.options.privateFolders")} aside={t("permissions.privacy.privateHint")} />
      <ListEditor
        label={t("permissions.options.privateFolders")}
        items={strings(settings.tools?.["private-folders"])}
        onChange={(v) => save({ tools: { "private-folders": v } })}
        placeholder={t("permissions.options.folderPlaceholder")}
      />
      <Dialog
        open={confirmNever}
        onOpenChange={setConfirmNever}
        title={t("permissions.privacy.neverTitle")}
        description={t("permissions.privacy.neverBody")}
        footer={
          <>
            <Button onClick={() => setConfirmNever(false)}>{t("permissions.cancel")}</Button>
            <Button
              variant="destructive"
              onClick={() => {
                save({ privacy: { "retention-days": 0 } });
                setConfirmNever(false);
              }}
            >
              {t("permissions.privacy.neverConfirm")}
            </Button>
          </>
        }
      />
    </>
  );
}
