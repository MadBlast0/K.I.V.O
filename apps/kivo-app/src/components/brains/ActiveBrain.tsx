/**
 * The brain KIVO uses (owner, 2026-09-26): any number of brains can be connected (API services,
 * apps on this PC, local servers); this picks the one that answers, its model, and how hard it
 * thinks when the model can take a level. "Automatic" lets KIVO choose among them (BRAINS §5).
 * Shown in setup's Think page and at the top of the Brains page.
 */
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { brainColor, monogram, type BrainsList, type Effort } from "../../ipc/brains";
import { Method } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { Explain, Monogram, Segmented, Select, useToast } from "../ui";

const AUTO = "auto";
const DEFAULT = "default";
const LEVELS: ReadonlyArray<Effort> = ["off", "low", "medium", "high"];
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function ActiveBrain({ list, onChange }: { list: BrainsList; onChange: (list: BrainsList) => void }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const active = list.active;
  const brain = list.connected.find((b) => b.id === active?.provider);
  const model = active?.model || brain?.models[0] || "";
  const [levels, setLevels] = useState<Effort[]>([]);
  useEffect(() => {
    if (!active) return;
    let stale = false;
    void request<Effort[]>(Method.brainsReasoning, { provider: active.provider, model })
      .then((l) => !stale && setLevels(l))
      .catch(() => !stale && setLevels([]));
    return () => {
      stale = true;
    };
  }, [active, model, request]);

  const set = (provider: string | null, next: string, reasoning?: Effort) =>
    request<BrainsList>(Method.brainsSetActive, { provider, model: next, reasoning: reasoning ?? null })
      .then(onChange)
      .catch((e: unknown) => toast(message(e)));

  if (list.connected.length === 0) {
    return <p className="k-active k-active--empty">{t("brains.active.none")}</p>;
  }
  return (
    <div className="k-active">
      <div className="k-active__row">
        {brain ? (
          <Monogram text={monogram(brain.name)} color={brainColor(brain.id)} />
        ) : (
          <span className="k-active__auto" aria-hidden>
            A
          </span>
        )}
        <div className="k-active__text">
          <b>{t("brains.active.title")}</b>
          <span>{brain ? t("brains.active.chosen") : t("brains.active.auto")}</span>
        </div>
        <Select<string>
          label={t("brains.active.brain")}
          value={active?.provider ?? AUTO}
          onChange={(v) => void set(v === AUTO ? null : v, list.connected.find((b) => b.id === v)?.models[0] ?? "")}
          items={[
            { value: AUTO, label: t("brains.active.automatic") },
            ...list.connected.map((b) => ({ value: b.id, label: b.name })),
          ]}
        />
      </div>
      {brain && brain.models.length > 0 && (
        <div className="k-active__row k-active__row--sub">
          <span className="k-active__label">{t("brains.active.model")}</span>
          <Select<string>
            label={t("brains.active.model")}
            value={model}
            onChange={(v) => void set(brain.id, v, active?.reasoning)}
            items={brain.models.slice(0, 200).map((m) => ({ value: m, label: m }))}
          />
        </div>
      )}
      {brain && levels.length > 0 && (
        <div className="k-active__row k-active__row--sub">
          <span className="k-active__label">
            <Explain tip={t("brains.active.reasoningTip")}>{t("brains.active.reasoning")}</Explain>
          </span>
          <Segmented<string>
            label={t("brains.active.reasoning")}
            value={active?.reasoning ?? DEFAULT}
            onChange={(v) =>
              void set(
                brain.id,
                model,
                LEVELS.find((l) => l === v),
              )
            }
            options={[DEFAULT, ...LEVELS.filter((l) => levels.includes(l))].map((l) => ({
              value: l,
              label: t(`brains.active.level.${l}`),
            }))}
          />
        </div>
      )}
    </div>
  );
}
