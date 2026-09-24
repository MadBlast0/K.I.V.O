/**
 * Voice (UX §3, UX-23, UX-61, UX-62): how KIVO hears and speaks — recognition and speaking at a
 * glance (engine, status, language, live transcript, microphone, voice, preview, speed), profile
 * cards with the recommendation for this PC and safe switching (VOICE §11), the voice cards with
 * previews — plus wake words, follow-ups, the owner's voice, KIVO's personality, the user's own
 * words, the speech models on this PC (DIST-13) with each licence shown before anything
 * downloads (download, pause, cancel, remove, set as default, update), and the advanced view
 * (VOICE-49). Lists update from the runtime's pushed events.
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
import { AdvancedVoice, Personality, SpeechSummary, Vocabulary } from "../components/voice/Details";
import { Enrollment, SpeakerMode } from "../components/voice/Enrollment";
import { CloudEngines } from "../components/voice/CloudEngines";
import { Recommended, SpeechChooser, VoiceList } from "../components/voice/SpeechChooser";
import { useSpeech } from "../components/voice/useSpeech";
import { WakeWords } from "../components/voice/WakeWords";
import { Method, type ModelItem } from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";

/** Follow-up window choices in seconds (UX-45: Off / 5 / 8 / 15). */
const FOLLOW_UP = ["0", "5", "8", "15"] as const;

function FollowUp() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const [seconds, setSeconds] = useState<string | null>(null);
  const connected = link?.status === "connected";
  useEffect(() => {
    if (!connected) return;
    void request<{ voice: Record<string, unknown> }>(Method.settingsGet)
      .then((s) => {
        const value = s.voice["follow-up-seconds"];
        setSeconds(typeof value === "number" ? String(value) : "8");
      })
      .catch(() => {});
  }, [connected, request]);
  if (seconds === null) return null;
  return (
    <Group>
      <Row
        icon="repeat"
        title={t("voice.followUp")}
        subtitle={t("voice.followUpHint")}
        end={
          <Select
            label={t("voice.followUp")}
            value={seconds}
            onChange={(v) => {
              request(Method.settingsSet, { voice: { "follow-up-seconds": Number(v) } })
                .then(() => setSeconds(v))
                .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));
            }}
            items={FOLLOW_UP.map((value) => ({
              value,
              label: value === "0" ? t("voice.followUpOff") : t("voice.seconds", { count: Number(value) }),
            }))}
          />
        }
      />
    </Group>
  );
}

/** A model's state and what can be done with it (UX-61). */
function ModelEnd({
  model: m,
  onRemove,
  onDownload,
  act,
}: {
  model: ModelItem;
  onRemove: () => void;
  onDownload: () => void;
  act: (method: Method, id: string) => void;
}) {
  const { t } = useTranslation();
  const speechEngine = m.kind === "stt" || m.kind === "tts";
  switch (m.state) {
    case "downloading":
    case "installing":
      return (
        <span className="k-voice__model-end">
          <Tag>
            {m.state === "installing"
              ? t("voice.state.installing")
              : t("voice.downloading", { percent: m.downloading ?? 0 })}
          </Tag>
          {m.state === "downloading" && (
            <IconButton
              icon="pause"
              size="sm"
              label={t("voice.pause", { name: m.name })}
              onClick={() => act(Method.modelsPause, m.id)}
            />
          )}
          <IconButton
            icon="close"
            size="sm"
            label={t("voice.cancelDownload", { name: m.name })}
            onClick={() => act(Method.modelsCancel, m.id)}
          />
        </span>
      );
    case "paused":
      return (
        <span className="k-voice__model-end">
          <Tag tone="warning">{t("voice.state.paused")}</Tag>
          <Button size="sm" icon="play" onClick={() => act(Method.modelsInstall, m.id)}>
            {t("voice.resume")}
          </Button>
          <IconButton
            icon="close"
            size="sm"
            label={t("voice.cancelDownload", { name: m.name })}
            onClick={() => act(Method.modelsCancel, m.id)}
          />
        </span>
      );
    case "error":
      return (
        <span className="k-voice__model-end">
          <Tag tone="danger">{t("voice.state.error")}</Tag>
          <Button size="sm" icon="refresh" onClick={() => act(Method.modelsInstall, m.id)}>
            {t("voice.retry")}
          </Button>
        </span>
      );
    case "ready":
    case "updateAvailable":
      return (
        <span className="k-voice__model-end">
          {m.state === "updateAvailable" && (
            <Button size="sm" icon="download" onClick={() => act(Method.modelsInstall, m.id)}>
              {t("voice.update")}
            </Button>
          )}
          {m.residency && m.residency !== "unloaded" && (
            // Loaded in memory right now (PLAN-02); it unloads after going unused.
            <Tag tone={m.residency === "active" ? "success" : "neutral"}>{t(`voice.residency.${m.residency}`)}</Tag>
          )}
          {m.inUse ? (
            <Tag tone="success">{t("voice.state.inUse")}</Tag>
          ) : speechEngine ? (
            <Button size="sm" onClick={() => act(Method.modelsSetDefault, m.id)}>
              {t("voice.setDefault")}
            </Button>
          ) : (
            <Tag>{t("voice.state.ready")}</Tag>
          )}
          <IconButton icon="delete" label={t("voice.remove", { name: m.name })} onClick={onRemove} />
        </span>
      );
    default:
      return (
        <Button size="sm" icon="download" onClick={onDownload}>
          {t("voice.download")}
        </Button>
      );
  }
}

function Models() {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [models, setModels] = useState<ModelItem[]>([]);
  const [offer, setOffer] = useState<ModelItem | null>(null);
  const [removing, setRemoving] = useState<ModelItem | null>(null);

  const load = useCallback(() => {
    if (!connected) return;
    void request<ModelItem[]>(Method.modelsList)
      .then(setModels)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  useRuntimeEvents((event) => {
    if (event.group === "system" && (event.event.type === "modelChanged" || event.event.type === "modelResidency")) {
      load();
    }
  });

  const fail = (e: unknown) => toast(e instanceof Error ? e.message : String(e));
  const act = (method: Method, id: string) => void request(method, { id }).then(load).catch(fail);
  const download = () => {
    if (!offer) return;
    const model = offer;
    setOffer(null);
    void request(Method.modelsInstall, { id: model.id }).catch(fail);
  };
  const remove = () => {
    if (!removing) return;
    const model = removing;
    setRemoving(null);
    // The dialog said what removing it means, so it is confirmed (VOICE-45).
    void request(Method.modelsRemove, { id: model.id, confirmed: true }).catch(fail);
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

  return (
    <>
      <Group>
        {models.map((m) => (
          <Row
            key={m.id}
            icon={m.kind === "stt" ? "mic" : m.kind === "tts" ? "volume" : "cpu"}
            title={m.name}
            subtitle={
              m.error ??
              t("voice.modelLine", {
                license: m.license,
                size: m.installed ? size(m.diskBytes) : size(m.size),
                where: m.installed ? t("voice.onThisPc") : t("voice.toDownload"),
              })
            }
            end={<ModelEnd model={m} onRemove={() => setRemoving(m)} onDownload={() => setOffer(m)} act={act} />}
          />
        ))}
      </Group>

      <Dialog
        open={offer !== null}
        onOpenChange={(open) => !open && setOffer(null)}
        title={offer ? t("voice.downloadTitle", { name: offer.name }) : ""}
        description={offer ? t("voice.downloadSize", { size: size(offer.size) }) : ""}
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
              <b>{t("voice.license")}</b> {offer.license}
            </p>
            <p>{offer.attribution}</p>
            <p className="k-licence__source">{offer.source}</p>
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

export function Voice() {
  const { t } = useTranslation();
  const speech = useSpeech();
  const [choosing, setChoosing] = useState(false);

  if (!speech.connected) {
    return (
      <>
        <PageHeader title={t("nav.voice")} subtitle={t("voice.subtitle")} />
        <Note>{t("voice.notConnected")}</Note>
      </>
    );
  }

  return (
    <>
      <PageHeader title={t("nav.voice")} subtitle={t("voice.subtitle")} />
      <SpeechSummary speech={speech} onChange={() => setChoosing((c) => !c)} />
      {choosing && (
        <>
          <Recommended speech={speech} />
          <Section title={t("speech.sttTitle")} aside={t("speech.sttHint")} />
          <SpeechChooser slot="stt" speech={speech} />
          <CloudEngines slot="stt" speech={speech} />
          <Section title={t("speech.ttsTitle")} aside={t("speech.ttsHint")} />
          <SpeechChooser slot="tts" speech={speech} />
          <CloudEngines slot="tts" speech={speech} />
        </>
      )}
      <VoiceList speech={speech} />
      <Personality />

      <Section title={t("wake.title")} aside={t("wake.hint")} />
      <WakeWords />
      <FollowUp />

      <Section title={t("enroll.title")} aside={t("enroll.hint")} />
      <Enrollment />
      <SpeakerMode />

      <Vocabulary />

      <Section title={t("voice.models")} aside={t("voice.modelsHint")} />
      <Models />
      <AdvancedVoice />
    </>
  );
}
