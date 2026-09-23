/**
 * Voice (UX §3; the M1 part of UX-23): which voice speaks KIVO's replies, and the speech models on
 * this PC (DIST-13): each with its licence, shown before anything downloads, its size on disk and
 * Remove. The list updates as downloads move, from the runtime's pushed events (DISC-13).
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import {
  Button,
  Dialog,
  DialogClose,
  Group,
  IconButton,
  Note,
  Row,
  Section,
  Select,
  Tag,
  useToast,
} from "../components/ui";
import { Method, type ModelItem } from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";

const SYSTEM_VOICE = "system";
const KOKORO = "kokoro-82m";

export function Voice() {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [models, setModels] = useState<ModelItem[]>([]);
  const [engine, setEngine] = useState(SYSTEM_VOICE);
  // A model whose licence is on screen before it downloads; `useAsVoice` picks it as the voice after.
  const [offer, setOffer] = useState<{ model: ModelItem; useAsVoice: boolean } | null>(null);
  const [removing, setRemoving] = useState<ModelItem | null>(null);

  const load = useCallback(() => {
    if (!connected) return;
    void request<ModelItem[]>(Method.modelsList)
      .then(setModels)
      .catch(() => {});
    void request<{ voice: Record<string, unknown> }>(Method.settingsGet)
      .then((settings) => {
        const chosen = settings.voice["tts-engine"];
        setEngine(typeof chosen === "string" && chosen ? chosen : SYSTEM_VOICE);
      })
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  useRuntimeEvents((event) => {
    if (event.group === "system" && (event.event.type === "modelChanged" || event.event.type === "modelResidency")) {
      load();
    }
  });

  const fail = (e: unknown) => toast(e instanceof Error ? e.message : String(e));
  const pickVoice = (id: string) =>
    request(Method.settingsSet, { voice: { "tts-engine": id } })
      .then(() => setEngine(id))
      .catch(fail);

  const chooseEngine = (id: string) => {
    const model = models.find((m) => m.id === id);
    if (model && !model.installed && model.downloading === null) {
      setOffer({ model, useAsVoice: true });
    } else {
      void pickVoice(id);
    }
  };

  const download = () => {
    if (!offer) return;
    const { model, useAsVoice } = offer;
    setOffer(null);
    void request(Method.modelsInstall, { id: model.id })
      .then(() => (useAsVoice ? pickVoice(model.id) : undefined))
      .catch(fail);
  };

  const remove = () => {
    if (!removing) return;
    const model = removing;
    setRemoving(null);
    void request(Method.modelsRemove, { id: model.id })
      .then(() => (model.id === engine ? pickVoice(SYSTEM_VOICE) : undefined))
      .catch(fail);
  };

  const megabytes = useMemo(
    () =>
      new Intl.NumberFormat(i18n.language, {
        style: "unit",
        unit: "megabyte",
        maximumFractionDigits: 0,
      }),
    [i18n.language],
  );
  const size = (bytes: number) => megabytes.format(Math.max(1, Math.round(bytes / 1_000_000)));

  const engines = [
    { value: SYSTEM_VOICE, label: t("voice.systemVoices") },
    { value: KOKORO, label: t("voice.kokoro") },
  ];

  return (
    <>
      <PageHeader title={t("nav.voice")} subtitle={t("voice.subtitle")} />

      <Section title={t("voice.speaking")} />
      <Group>
        <Row
          icon="volume"
          title={t("voice.engine")}
          subtitle={t("voice.engineHint")}
          end={
            connected ? (
              <Select label={t("voice.engine")} items={engines} value={engine} onChange={chooseEngine} />
            ) : null
          }
        />
      </Group>

      <Section title={t("voice.models")} aside={t("voice.modelsHint")} />
      {connected ? (
        <Group>
          {models.map((m) => (
            <Row
              key={m.id}
              icon={m.kind === "stt" ? "mic" : "volume"}
              title={m.name}
              subtitle={t("voice.modelLine", {
                license: m.license,
                size: m.installed ? size(m.diskBytes) : size(m.size),
                where: m.installed ? t("voice.onThisPc") : t("voice.toDownload"),
              })}
              end={
                m.downloading !== null ? (
                  <Tag>{t("voice.downloading", { percent: m.downloading })}</Tag>
                ) : m.installed ? (
                  <span className="k-voice__model-end">
                    {m.residency && m.residency !== "unloaded" && (
                      // Loaded in memory right now (PLAN-02); it unloads after going unused.
                      <Tag tone={m.residency === "active" ? "success" : "neutral"}>
                        {t(`voice.residency.${m.residency}`)}
                      </Tag>
                    )}
                    <IconButton
                      icon="delete"
                      label={t("voice.remove", { name: m.name })}
                      onClick={() => setRemoving(m)}
                    />
                  </span>
                ) : (
                  <Button size="sm" icon="download" onClick={() => setOffer({ model: m, useAsVoice: false })}>
                    {t("voice.download")}
                  </Button>
                )
              }
            />
          ))}
        </Group>
      ) : (
        <Note>{t("voice.notConnected")}</Note>
      )}

      <Dialog
        open={offer !== null}
        onOpenChange={(open) => !open && setOffer(null)}
        title={offer ? t("voice.downloadTitle", { name: offer.model.name }) : ""}
        description={offer ? t("voice.downloadSize", { size: size(offer.model.size) }) : ""}
        footer={
          <>
            <DialogClose>
              <Button>{t("voice.cancel")}</Button>
            </DialogClose>
            <Button variant="primary" icon="download" onClick={download}>
              {t("voice.download")}
            </Button>
          </>
        }
      >
        {offer && (
          <div className="k-licence">
            <p>
              <b>{t("voice.license")}</b> {offer.model.license}
            </p>
            <p>{offer.model.attribution}</p>
            <p className="k-licence__source">{offer.model.source}</p>
          </div>
        )}
      </Dialog>

      <Dialog
        open={removing !== null}
        onOpenChange={(open) => !open && setRemoving(null)}
        title={removing ? t("voice.removeTitle", { name: removing.name }) : ""}
        description={
          removing
            ? removing.kind === "stt"
              ? t("voice.removeHearing", { size: size(removing.diskBytes) })
              : t("voice.removeVoice", { size: size(removing.diskBytes) })
            : ""
        }
        footer={
          <>
            <DialogClose>
              <Button>{t("voice.cancel")}</Button>
            </DialogClose>
            <Button variant="destructive" icon="delete" onClick={remove}>
              {t("voice.removeButton")}
            </Button>
          </>
        }
      />
    </>
  );
}
