/**
 * Settings → Performance (UX-31, PLAN-07, DISC-18): the performance profile; how much of the PC KIVO
 * uses right now (its processes' CPU over a second and their memory, the graphics card's load and
 * KIVO's share of its memory, the PC's memory, and the NPU), refreshed every 2 s only while this
 * tab is open; response-time medians from the latest requests (ARCH-28); and which speech models
 * are loaded (PLAN-02).
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Group, Meta, Note, Row, Section, Select, Stat, Tag, type Tone } from "../../components/ui";
import { Method } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { oneOf } from "../../lib/settings";
import { useConfig } from "./useConfig";

type Profile = "auto" | "low-resource" | "battery" | "balanced" | "performance" | "gaming" | "custom";
const PROFILES: ReadonlyArray<Profile> = [
  "auto",
  "low-resource",
  "battery",
  "balanced",
  "performance",
  "gaming",
  "custom",
];

interface Status {
  cpuPercent: number;
  memoryMb: number;
  processes: number;
  gpu: boolean;
  gpuPercent: number | null;
  gpuMemoryMb: number | null;
  vramMb: number | null;
  ramMb: number | null;
  ramFreeMb: number | null;
  npu: boolean;
  profile: Profile;
  /** Auto's choice right now (PLAN-08). */
  effectiveProfile?: Profile;
  latency: {
    wakeToChime: number | null;
    speechToText: number | null;
    commandDone: number | null;
    firstWord: number | null;
    /** Each stage on its own (PLAN-07). */
    stages?: {
      stt: number | null;
      brain: number | null;
      tool: number | null;
      tts: number | null;
      total: number | null;
    };
  };
  models: { id: string; name: string; kind: string; residency: string | null; diskBytes: number }[];
}

const RESIDENCY_TONE: Record<string, Tone> = { active: "success", warm: "success", warming: "warning" };

export function PerformanceTab() {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const { get, set } = useConfig();
  const [status, setStatus] = useState<Status | null>(null);
  const [busy, setBusy] = useState(false);
  const connected = link?.status === "connected";
  const load = useCallback(
    () =>
      connected
        ? request<Status>(Method.performanceStatus)
            .then(setStatus)
            .catch(() => {})
        : Promise.resolve(),
    [connected, request],
  );
  // Sampled every 2 s while this tab is open, and not at all otherwise (DISC-18).
  useEffect(() => {
    void load();
    const timer = window.setInterval(() => void load(), 2_000);
    return () => window.clearInterval(timer);
  }, [load]);
  const measure = () => {
    setBusy(true);
    void load().finally(() => setBusy(false));
  };

  const ms = (v: number | null) =>
    v === null
      ? t("settings.performance.notYet")
      : v >= 1000
        ? t("settings.performance.seconds", {
            value: new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 }).format(v / 1000),
          })
        : t("settings.performance.ms", { value: v });

  return (
    <>
      <Group>
        <Row
          icon="speed"
          title={t("settings.performance.profile")}
          subtitle={
            (oneOf(get("performance", "profile"), PROFILES) ?? "auto") === "auto" && status?.effectiveProfile
              ? t("settings.performance.autoChose", {
                  profile: t(`settings.performance.profiles.${status.effectiveProfile}`),
                })
              : t(`settings.performance.profileHints.${oneOf(get("performance", "profile"), PROFILES) ?? "auto"}`)
          }
          end={
            <Select<Profile>
              label={t("settings.performance.profile")}
              value={oneOf(get("performance", "profile"), PROFILES) ?? "auto"}
              onChange={(v) => set("performance", { profile: v })}
              items={PROFILES.map((value) => ({ value, label: t(`settings.performance.profiles.${value}`) }))}
            />
          }
        />
      </Group>

      <Section
        title={t("settings.performance.now")}
        aside={
          <Button size="sm" variant="plain" icon="refresh" disabled={busy} onClick={measure}>
            {t("settings.performance.measure")}
          </Button>
        }
      />
      <div className="k-grid k-grid--4">
        <Stat
          value={
            status
              ? `${new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 }).format(status.cpuPercent)}%`
              : "—"
          }
          label={t("settings.performance.cpu")}
        />
        <Stat value={status ? `${status.memoryMb} MB` : "—"} label={t("settings.performance.memory")} />
        <Stat
          value={
            !status
              ? "—"
              : status.gpu
                ? t("settings.performance.gpuKivo", { mb: status.gpuMemoryMb ?? 0 })
                : t("settings.performance.gpuNone")
          }
          label={t("settings.performance.gpu")}
        />
        <Stat value={status ? ms(status.latency.wakeToChime) : "—"} label={t("settings.performance.wake")} />
      </div>
      <Group>
        <Row
          icon="monitor"
          title={t("settings.performance.gpuLoad")}
          subtitle={
            status?.vramMb != null
              ? t("settings.performance.vram", { mb: status.vramMb })
              : t("settings.performance.noGpu")
          }
          end={<Meta>{status?.gpuPercent != null ? `${status.gpuPercent}%` : "—"}</Meta>}
        />
        <Row
          icon="cpu"
          title={t("settings.performance.ram")}
          end={
            <Meta>
              {status?.ramMb != null && status.ramFreeMb != null
                ? t("settings.performance.ramFree", {
                    free: Math.round(status.ramFreeMb / 1024),
                    total: Math.round(status.ramMb / 1024),
                  })
                : "—"}
            </Meta>
          }
        />
        <Row
          icon="speed"
          title={t("settings.performance.npu")}
          end={
            <Meta>
              {status ? (status.npu ? t("settings.performance.npuYes") : t("settings.performance.npuNo")) : "—"}
            </Meta>
          }
        />
      </Group>
      <Note>{t("settings.performance.liveNote")}</Note>

      <Section title={t("settings.performance.times")} />
      <Group>
        {(["wakeToChime", "speechToText", "commandDone", "firstWord"] as const).map((k) => (
          <Row
            key={k}
            title={t(`settings.performance.latency.${k}`)}
            end={<Meta>{status ? ms(status.latency[k]) : "—"}</Meta>}
          />
        ))}
      </Group>
      <Section title={t("settings.performance.stages")} />
      <Group>
        {(["stt", "brain", "tool", "tts", "total"] as const).map((k) => (
          <Row
            key={k}
            title={t(`settings.performance.stage.${k}`)}
            end={<Meta>{status?.latency.stages ? ms(status.latency.stages[k]) : "—"}</Meta>}
          />
        ))}
      </Group>
      <Note>{t("settings.performance.timesNote")}</Note>

      <Section title={t("settings.performance.models")} />
      <Group>
        {!status || status.models.length === 0 ? (
          <Row title={t("settings.performance.noModels")} />
        ) : (
          status.models.map((m) => {
            const r = m.residency ?? "unloaded";
            return (
              <Row
                key={m.id}
                icon={m.kind === "tts" ? "wave" : "mic"}
                title={m.name}
                subtitle={t("settings.performance.onDisk", { mb: Math.round(m.diskBytes / (1024 * 1024)) })}
                end={
                  <Tag tone={RESIDENCY_TONE[r] ?? "neutral"}>
                    {t(`settings.performance.residency.${r}`, { defaultValue: r })}
                  </Tag>
                }
              />
            );
          })
        )}
      </Group>
    </>
  );
}
