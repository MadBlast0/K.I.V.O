/**
 * Brains → Context (CONV-30/31, CONVERSATION §8): what a conversation starts with, layer by layer —
 * each layer's size, a switch to leave it out and a link to edit it; the live fields sent; auto-
 * compaction and its threshold; "Start each conversation fresh"; the budget and an estimated cost
 * of a new conversation for the current brain; and "Preview what the AI sees", the whole request
 * with secrets redacted. Advanced: the defaults just work.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Button,
  Checkbox,
  Group,
  IconButton,
  Note,
  Row,
  Section,
  Select,
  Sheet,
  Spinner,
  Switch,
  TextField,
  useToast,
} from "../ui";
import type { ContextLayer, ContextPreview } from "../../ipc/brains";
import { Method } from "../../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../../ipc/runtime";
import { bool, num, strings } from "../../lib/settings";
import { message, useConfig } from "../../pages/settings/useConfig";

const COLORS: Record<string, string> = {
  system: "var(--text)",
  instructions: "var(--acc)",
  workspace: "#5E5CE6",
  live: "#8E8E93",
  skills: "#BF5AF2",
  memories: "#64D2FF",
  summary: "#32D74B",
  tools: "#FF9F0A",
};
const LIVE_FIELDS = ["local time", "language", "permission mode", "active app", "window title"] as const;
const THRESHOLDS = ["60", "70", "80", "90", "100"] as const;
/** The layers a switch can leave out, and their setting. */
const SWITCHABLE: Partial<Record<ContextLayer["id"], string>> = {
  instructions: "about-me",
  workspace: "workspace",
  memories: "memories",
  skills: "skills",
};

export function ContextSettings({ onNavigate }: { onNavigate?: (to: string) => void }) {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const fail = (e: unknown) => toast(message(e));
  const { get, set } = useConfig();
  const connected = link?.status === "connected";
  const [preview, setPreview] = useState<ContextPreview | null>(null);
  const [prefs, setPrefs] = useState<{ key: string; value: string }[]>([]);
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");
  const [viewing, setViewing] = useState<{ title: string; text: string } | null>(null);
  const load = useCallback(() => {
    if (!connected) return;
    void request<ContextPreview>(Method.brainsContext)
      .then(setPreview)
      .catch(() => {});
    void request<{ key: string; value: string }[]>(Method.memoryPreferences)
      .then(setPrefs)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  // A setting changed (here or anywhere): the sizes and the preview follow.
  useRuntimeEvents((event) => {
    if (event.group === "system" && event.event.type === "configChanged") load();
  });
  const number = new Intl.NumberFormat(i18n.language);
  const money = new Intl.NumberFormat(i18n.language, {
    style: "currency",
    currency: "USD",
    maximumSignificantDigits: 2,
  });
  if (!connected) return <Note>{t("brains.notConnected")}</Note>;
  if (!preview) return <Spinner label={t("brains.loading")} />;

  const saveContext = (patch: Record<string, unknown>) => set("context", patch);
  const shown = preview.layers.filter((l) => l.id !== "turns" && l.on !== false);
  // What every conversation starts with: tools, memories and the summary come only when relevant.
  const start = shown
    .filter((l) => l.id !== "tools" && l.id !== "memories" && l.id !== "summary")
    .reduce((sum, l) => sum + l.tokens, 0);
  const total = shown.reduce((sum, l) => sum + l.tokens, 0);
  const live = strings(get("context", "live-fields"));
  const liveOn = get("context", "live-fields") === undefined ? [...LIVE_FIELDS] : live;
  const layer = (id: ContextLayer["id"]) => preview.layers.find((l) => l.id === id);

  const layerRow = (id: ContextLayer["id"], icon: Parameters<typeof Row>[0]["icon"], edit?: () => void) => {
    const l = layer(id);
    if (!l) return null;
    const setting = SWITCHABLE[id];
    const on = l.on !== false;
    return (
      <Row
        key={id}
        icon={icon}
        title={t(`context.layer.${id}`)}
        subtitle={t(`context.hint.${id}`, { count: l.count ?? 0 })}
        end={
          <>
            {l.max !== undefined && on && (
              <span className="k-meta">{t("context.tokensOf", { tokens: l.tokens, max: l.max })}</span>
            )}
            {l.text !== undefined && on && l.tokens > 0 && (
              <Button
                size="sm"
                variant="plain"
                onClick={() => setViewing({ title: t(`context.layer.${id}`), text: l.text ?? "" })}
              >
                {t("context.view")}
              </Button>
            )}
            {edit && (
              <Button size="sm" variant="plain" onClick={edit}>
                {t("context.edit")}
              </Button>
            )}
            {setting && (
              <Switch label={t(`context.layer.${id}`)} checked={on} onChange={(v) => saveContext({ [setting]: v })} />
            )}
          </>
        }
      />
    );
  };

  return (
    <div className="k-brains">
      <div className="k-context__tile">
        <b>{t("context.start", { tokens: number.format(start) })}</b>
        <span className="k-meta">
          {preview.brain
            ? t("context.budget", {
                brain: preview.brain,
                voice: number.format(preview.budget.voice),
                chat: number.format(preview.budget.chat),
              })
            : t("context.noBrain")}
        </span>
        {preview.brain && (
          <span className="k-meta">
            {preview.sessionCost == null
              ? t("context.costUnknown")
              : preview.sessionCost === 0
                ? t("context.costFree")
                : t("context.cost", { cost: money.format(preview.sessionCost) })}
          </span>
        )}
        <div className="k-context__bar" aria-hidden>
          {shown.map((l) => (
            <i key={l.id} style={{ width: `${(l.tokens / Math.max(1, total)) * 100}%`, background: COLORS[l.id] }} />
          ))}
        </div>
        <div className="k-context__legend">
          {shown.map((l) => (
            <span key={l.id}>
              <b style={{ background: COLORS[l.id] }} />
              {t(`context.layer.${l.id}`)} {number.format(l.tokens)}
            </span>
          ))}
        </div>
        <div className="k-context__actions">
          <Button
            icon="eye"
            disabled={!preview.preview}
            onClick={() => setViewing({ title: t("context.preview"), text: preview.preview ?? "" })}
          >
            {t("context.preview")}
          </Button>
          <span className="k-meta">{t("context.previewHint")}</span>
        </div>
      </div>

      <Section title={t("context.always")} />
      <Group>
        {layerRow("system", "file")}
        {layerRow("instructions", "user", () => onNavigate?.("memory"))}
        {layerRow("workspace", "folder", () => onNavigate?.("memory"))}
        {layerRow("live", "compass")}
        {layerRow("skills", "skill", () => onNavigate?.("extensions/skills"))}
      </Group>
      <div className="k-context__fields" role="group" aria-label={t("context.liveFields")}>
        <span className="k-meta">{t("context.liveFields")}</span>
        {LIVE_FIELDS.map((f) => (
          <Checkbox
            key={f}
            checked={liveOn.includes(f)}
            onChange={(v) =>
              saveContext({
                "live-fields": v ? [...liveOn.filter((x) => x !== f), f] : liveOn.filter((x) => x !== f),
              })
            }
          >
            {t(`context.field.${f.replace(/ /g, "-")}`)}
          </Checkbox>
        ))}
      </div>

      <Section title={t("context.onlyWhenRelevant")} />
      <Group>
        {layerRow("memories", "memory", () => onNavigate?.("memory"))}
        {layerRow("summary", "compress")}
        {layerRow("tools", "connector", () => onNavigate?.("permissions/capabilities"))}
      </Group>

      <Section title={t("context.conversation")} />
      <Group>
        <Row
          icon="compress"
          title={t("context.autoCompact")}
          subtitle={t("context.autoCompactHint")}
          end={
            <Switch
              label={t("context.autoCompact")}
              checked={bool(get("context", "auto-compact"), true)}
              onChange={(v) => saveContext({ "auto-compact": v })}
            />
          }
        />
        <Row
          icon="speed"
          title={t("context.compactAt")}
          subtitle={t("context.compactAtHint")}
          end={
            <Select
              label={t("context.compactAt")}
              value={String(num(get("context", "compact-at"), 100))}
              onChange={(v) => saveContext({ "compact-at": Number(v) })}
              items={THRESHOLDS.map((p) => ({
                value: p,
                label: p === "100" ? t("context.whenFull") : t("context.percent", { percent: p }),
              }))}
            />
          }
        />
        <Row
          icon="refresh"
          title={t("context.fresh")}
          subtitle={t("context.freshHint")}
          end={
            <Switch
              label={t("context.fresh")}
              checked={bool(get("context", "fresh-start"))}
              onChange={(v) => saveContext({ "fresh-start": v })}
            />
          }
        />
        <Row
          icon="brain"
          title={t("context.profileBudget")}
          subtitle={t("context.profileBudgetHint")}
          end={
            <Button size="sm" variant="plain" onClick={() => onNavigate?.("brains/brains")}>
              {t("context.edit")}
            </Button>
          }
        />
      </Group>

      <Section title={t("context.aboutMe")} />
      <Group>
        {prefs.map((p) => (
          <Row
            key={p.key}
            icon="user"
            title={p.key}
            subtitle={p.value}
            end={
              <IconButton
                size="sm"
                icon="delete"
                label={t("context.removePreference", { key: p.key })}
                onClick={() => void request(Method.memoryDeletePreference, { key: p.key }).then(load).catch(fail)}
              />
            }
          />
        ))}
      </Group>
      <div className="k-brains__key">
        <TextField
          value={key}
          placeholder={t("context.prefKey")}
          onChange={(e) => setKey(e.target.value)}
          aria-label={t("context.prefKey")}
        />
        <TextField
          value={value}
          placeholder={t("context.prefValue")}
          onChange={(e) => setValue(e.target.value)}
          aria-label={t("context.prefValue")}
        />
        <Button
          disabled={!key.trim() || !value.trim()}
          onClick={() =>
            void request(Method.memorySetPreference, { key, value })
              .then(() => {
                setKey("");
                setValue("");
                load();
              })
              .catch(fail)
          }
        >
          {t("context.addPreference")}
        </Button>
      </div>
      <Note>{t("context.note")}</Note>
      {viewing !== null && (
        <Sheet open onOpenChange={(o) => !o && setViewing(null)} title={viewing.title}>
          <pre className="k-context__text">{viewing.text || t("context.empty")}</pre>
        </Sheet>
      )}
    </div>
  );
}
