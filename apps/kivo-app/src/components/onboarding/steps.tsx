/**
 * Setup's steps 7–11 (UX §4, UX-35): connect apps and tools, the permission mode, look & feel,
 * startup, and Try it. The recommendation (UX-36, `setup.recommend`) preselects what fits this PC;
 * every choice is saved at once and can be changed on the same screen or later in Settings.
 */
import { useReducedMotionConfig } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Island, IslandSpin, type IslandModel } from "../island/Island";
import {
  AccentPicker,
  Button,
  Group,
  Keys,
  Monogram,
  Note,
  OptionCard,
  Pill,
  RadioGroup,
  Row,
  Section,
  Segmented,
  Select,
  Spinner,
  Switch,
  Tag,
  useToast,
} from "../ui";
import { brainColor, monogram } from "../../ipc/brains";
import { Method, type ConnectorView, type PermissionMode, type SetupAdvice } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { bool, oneOf, strings } from "../../lib/settings";
import { useTheme } from "../../lib/theme";
import { message, useConfig } from "../../pages/settings/useConfig";

/** Setup's recommendation for this PC, read once (UX-36). `null` until it arrives or if it can't. */
export function useAdvice(): SetupAdvice | null {
  const { link, request } = useRuntime();
  const [advice, setAdvice] = useState<SetupAdvice | null>(null);
  const connected = link?.status === "connected";
  useEffect(() => {
    if (!connected) return;
    void request<SetupAdvice>(Method.setupRecommend)
      .then(setAdvice)
      .catch(() => {});
  }, [connected, request]);
  return advice;
}

/** Why the recommendations are what they are, one line each. */
export function Reasons({ advice }: { advice: SetupAdvice | null }) {
  const { t } = useTranslation();
  if (!advice || advice.reasons.length === 0) return null;
  return (
    <div className="k-reasons" aria-label={t("onboarding.why")}>
      {advice.reasons.map((r) => (
        <span key={r}>
          <IconDot />
          {r}
        </span>
      ))}
    </div>
  );
}

const IconDot = () => <i className="k-reasons__dot" aria-hidden />;

/** Step 7: the apps and tools KIVO works with — ready ones, ones a sign-in away, and the rest. */
export function AppsStep({ onAfter }: { onAfter: (page: string) => void }) {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [list, setList] = useState<ConnectorView[] | null>(null);
  const [browser, setBrowser] = useState<{ connected: boolean; folder: string | null } | null>(null);
  const [how, setHow] = useState(false);
  const [later, setLater] = useState<string | null>(null);
  const load = () =>
    request<ConnectorView[]>(Method.connectorsList)
      .then(setList)
      .catch(() => setList([]));
  useEffect(() => {
    if (!connected) return;
    void load();
    void request<{ connected: boolean; folder: string | null }>(Method.browserStatus)
      .then(setBrowser)
      .catch(() => {});
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once, when connected
  }, [connected]);
  const connect = (id: string) =>
    void request(Method.connectorsConnect, { id })
      .then(load)
      .catch((e: unknown) => toast(message(e)));
  const ready = (list ?? []).filter((c) => c.state === "connected" || c.state === "ready");
  const signIn = (list ?? []).filter((c) => c.kind === "remote" && c.state !== "connected" && c.state !== "ready");
  const pick = (page: string) => {
    setLater(page);
    onAfter(page);
  };

  if (list === null) return <Spinner label={t("onboarding.apps.looking")} />;
  return (
    <>
      <Section title={t("onboarding.apps.ready")} />
      <Group>
        <Row
          icon="music"
          title={t("onboarding.apps.media")}
          subtitle={t("onboarding.apps.mediaHint")}
          end={<Tag tone="success">{t("onboarding.apps.on")}</Tag>}
        />
        {ready.map((c) => (
          <Row
            key={c.id}
            lead={<Monogram text={monogram(c.name)} color={brainColor(c.id)} />}
            title={c.name}
            subtitle={c.detail ?? c.access}
            end={<Tag tone="success">{t("onboarding.apps.on")}</Tag>}
          />
        ))}
      </Group>
      <Section title={t("onboarding.apps.signIn")} />
      <Group>
        {signIn.map((c) => (
          <Row
            key={c.id}
            lead={<Monogram text={monogram(c.name)} color={brainColor(c.id)} />}
            title={c.name}
            subtitle={c.access}
            end={
              c.state === "connecting" ? (
                <Spinner label={t("onboarding.apps.connecting")} />
              ) : (
                <Button size="sm" onClick={() => connect(c.id)}>
                  {t("onboarding.brain.connect")}
                </Button>
              )
            }
          />
        ))}
        <Row
          icon="globe"
          title={t("onboarding.apps.extension")}
          subtitle={
            browser?.connected
              ? t("onboarding.apps.extensionOn")
              : how && browser?.folder
                ? t("permissions.options.installHow", { folder: browser.folder })
                : t("onboarding.apps.extensionHint")
          }
          end={
            browser?.connected ? (
              <Tag tone="success">{t("onboarding.brain.connected")}</Tag>
            ) : (
              <Button size="sm" onClick={() => setHow((h) => !h)} aria-expanded={how}>
                {t("onboarding.apps.install")}
              </Button>
            )
          }
        />
      </Group>
      <Section title={t("onboarding.apps.advanced")} />
      <Group>
        <Row
          icon="server"
          title={t("onboarding.apps.mcp")}
          subtitle={t("onboarding.apps.mcpHint")}
          end={
            later === "extensions/mcp" ? (
              <Pill tone="accent">{t("onboarding.apps.afterSetup")}</Pill>
            ) : (
              <Button size="sm" variant="plain" onClick={() => pick("extensions/mcp")}>
                {t("onboarding.apps.add")}
              </Button>
            )
          }
        />
        <Row
          icon="skill"
          title={t("onboarding.apps.skills")}
          subtitle={t("onboarding.apps.skillsHint")}
          end={
            later === "extensions/skills" ? (
              <Pill tone="accent">{t("onboarding.apps.afterSetup")}</Pill>
            ) : (
              <Button size="sm" variant="plain" onClick={() => pick("extensions/skills")}>
                {t("onboarding.apps.import")}
              </Button>
            )
          }
        />
      </Group>
    </>
  );
}

const MODES: ReadonlyArray<PermissionMode> = ["ask", "accept-edits", "plan", "auto"];

/** Step 8: how much KIVO does on its own. Bypass isn't offered here (SEC-03). */
export function ModeStep({ advice }: { advice: SetupAdvice | null }) {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const current = link?.status === "connected" ? link.snapshot?.mode : undefined;
  const recommended = oneOf(advice?.mode, MODES) ?? "auto";
  return (
    <>
      <RadioGroup<PermissionMode>
        label={t("onboarding.mode.title")}
        value={current === "bypass" ? undefined : (current ?? recommended)}
        onChange={(m) => void request(Method.permissionsSetMode, { mode: m }).catch((e: unknown) => toast(message(e)))}
      >
        {MODES.map((m) => (
          <OptionCard
            key={m}
            value={m}
            title={t(`mode.${m}`)}
            description={t(`permissions.mode.${m}`)}
            badge={m === recommended ? <Pill tone="accent">{t("permissions.recommended")}</Pill> : undefined}
          />
        ))}
      </RadioGroup>
      <Note>{t("onboarding.mode.note")}</Note>
    </>
  );
}

/** A small desktop with a window and the Island on it (the mockup's `.mini`). */
export function MiniDesk({
  model,
  bottom = false,
  tall = false,
}: {
  model: IslandModel | null;
  bottom?: boolean;
  tall?: boolean;
}) {
  const { t } = useTranslation();
  return (
    <div className="k-mini" data-tall={tall || undefined} role="img" aria-label={t("onboarding.look.preview")}>
      <span className="k-mini__wall" />
      <span className="k-mini__app">
        <span className="k-mini__bar">{t("onboarding.look.notes")}</span>
      </span>
      <span className="k-mini__island" data-bottom={bottom || undefined}>
        <Island model={model} />
      </span>
    </div>
  );
}

/** Step 9: theme, accent, where the Island sits and the chimes, shown on a live Island. */
export function LookStep() {
  const { t } = useTranslation();
  const theme = useTheme();
  const { get, set } = useConfig();
  const position = oneOf(get("overlay", "position"), ["top-center", "bottom-center", "remember-drag"]) ?? "top-center";
  const chimes = bool(get("sounds", "enabled"), true);
  const demo: IslandModel = {
    state: "speaking",
    width: 300,
    label: t("onboarding.look.hello"),
    wave: true,
    voice: "kivo",
  };
  return (
    <>
      <MiniDesk model={demo} bottom={position === "bottom-center"} />
      <Group>
        <Row
          icon="theme"
          title={t("settings.appearance.theme")}
          end={
            <Segmented
              label={t("settings.appearance.theme")}
              value={theme.theme}
              onChange={theme.setTheme}
              options={(["light", "dark", "system"] as const).map((v) => ({
                value: v,
                label: t(`settings.appearance.themes.${v}`),
              }))}
            />
          }
        />
        <Row
          icon="palette"
          title={t("settings.appearance.accent")}
          end={<AccentPicker value={theme.accent} onChange={theme.setAccent} />}
        />
        <Row
          icon="island"
          title={t("settings.island.position")}
          end={
            <Segmented
              label={t("settings.island.position")}
              value={position === "bottom-center" ? "bottom-center" : "top-center"}
              onChange={(v) => set("overlay", { position: v })}
              options={[
                { value: "top-center", label: t("onboarding.look.top") },
                { value: "bottom-center", label: t("onboarding.look.bottom") },
              ]}
            />
          }
        />
        <Row
          icon="volume"
          title={t("onboarding.look.chimes")}
          subtitle={t("onboarding.look.chimesHint")}
          end={
            <Switch
              label={t("onboarding.look.chimes")}
              checked={chimes}
              onChange={(v) => set("sounds", { enabled: v })}
            />
          }
        />
      </Group>
    </>
  );
}

const PROFILES = ["auto", "low-resource", "battery", "balanced", "performance"] as const;
const PRIVACY = ["cloud", "local", "strict-private"] as const;

/** Step 10: open with Windows, keep running, and what this PC is suited to (UX-36). */
export function StartupStep({ advice }: { advice: SetupAdvice | null }) {
  const { t } = useTranslation();
  const { get, set } = useConfig();
  const profile = oneOf(get("performance", "profile"), PROFILES) ?? "auto";
  const privacy = oneOf(get("privacy", "mode"), PRIVACY);
  return (
    <>
      <Group>
        <Row
          icon="power"
          title={t("onboarding.startup.withWindows")}
          subtitle={t("onboarding.startup.withWindowsHint")}
          end={
            <Switch
              label={t("onboarding.startup.withWindows")}
              checked={bool(get("general", "start-with-windows"))}
              onChange={(v) => set("general", { "start-with-windows": v })}
            />
          }
        />
        <Row
          icon="tray"
          title={t("onboarding.startup.keepRunning")}
          subtitle={t("onboarding.startup.keepRunningHint")}
          end={
            <Switch
              label={t("onboarding.startup.keepRunning")}
              checked={bool(get("general", "keep-running-on-close"), true)}
              onChange={(v) => set("general", { "keep-running-on-close": v })}
            />
          }
        />
      </Group>
      <div className="k-tray-note">
        <span className="k-mark" aria-hidden />
        {t("onboarding.startup.tray")}
      </div>
      <Section title={t("onboarding.startup.forThisPc")} />
      <Group>
        <Row
          icon="speed"
          title={t("settings.performance.profile")}
          subtitle={advice?.performance === profile ? t("permissions.recommended") : undefined}
          end={
            <Select
              label={t("settings.performance.profile")}
              value={profile}
              onChange={(v) => set("performance", { profile: v })}
              items={PROFILES.map((p) => ({ value: p, label: t(`settings.performance.profiles.${p}`) }))}
            />
          }
        />
        <Row
          icon="privacy"
          title={t("onboarding.startup.privacy")}
          subtitle={privacy && advice?.privacy === privacy ? t("permissions.recommended") : undefined}
          end={
            <Select
              label={t("onboarding.startup.privacy")}
              value={privacy}
              onChange={(v) => set("privacy", { mode: v })}
              items={PRIVACY.map((p) => ({ value: p, label: t(`onboarding.startup.privacyModes.${p}`) }))}
            />
          }
        />
      </Group>
      <Reasons advice={advice} />
    </>
  );
}

/** Step 11: "You're all set" and a demo of a request on the Island. */
export function TryStep() {
  const { t, i18n } = useTranslation();
  const reduce = useReducedMotionConfig() ?? false;
  const { get } = useConfig();
  const keys = strings(get("voice", "push-to-talk"));
  const [model, setModel] = useState<IslandModel | null>(null);
  const timers = useRef<number[]>([]);
  useEffect(() => () => timers.current.forEach((id) => window.clearTimeout(id)), []);

  const play = () => {
    timers.current.forEach((id) => window.clearTimeout(id));
    const now = new Intl.DateTimeFormat(i18n.language, { hour: "numeric", minute: "2-digit" }).format(new Date());
    const question = t("onboarding.try.question");
    const heard = (text: string): IslandModel => ({
      state: "listening",
      width: text ? 420 : 236,
      label: t("island.listening"),
      wave: true,
      body: text ? <div>{text}</div> : undefined,
    });
    const steps: Array<[number, IslandModel | null]> = [
      [0, heard("")],
      [500, heard(t("onboarding.try.partial"))],
      [900, heard(question)],
      [1700, { state: "thinking", width: 196, label: t("island.thinking"), trail: <IslandSpin /> }],
      [
        2400,
        {
          state: "speaking",
          width: 420,
          label: t("demo.kivo"),
          wave: true,
          voice: "kivo",
          body: (
            <>
              <div className="k-island__quote">{question}</div>
              <div className="k-island__answer">{t("onboarding.try.answer", { time: now })}</div>
            </>
          ),
        },
      ],
      [5200, null],
    ];
    // Reduced motion: the answer only.
    const run = reduce ? steps.slice(4, 5) : steps;
    timers.current = run.map(([at, m]) => window.setTimeout(() => setModel(m), reduce ? 0 : at));
  };

  return (
    <>
      <MiniDesk model={model} tall />
      <div className="k-try">
        <Button icon="play" onClick={play}>
          {t("onboarding.try.show")}
        </Button>
        <span>
          {t("onboarding.try.orPress")} <Keys keys={keys.length ? keys : ["Ctrl", "Space"]} />
        </span>
      </div>
    </>
  );
}
