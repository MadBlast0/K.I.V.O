/**
 * Cloud speech services (VOICE-10/11): Deepgram, AssemblyAI and OpenAI to hear; Cartesia,
 * ElevenLabs, Azure, OpenAI and Deepgram to speak. Each needs the user's own key, which KIVO tests,
 * keeps in Windows Credential Manager and never shows again; each is labelled as sending audio or
 * text to the service, and the privacy mode can rule them out.
 */
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Group, Monogram, Row, Tag, TextField, useToast } from "../ui";
import { Method, type SpeechEngineItem } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { brainColor, monogram } from "../../ipc/brains";
import type { Speech } from "./useSpeech";

type Slot = "stt" | "tts";

function message(e: unknown) {
  return e instanceof Error ? e.message : String(e);
}

function KeyForm({ engine, onSaved }: { engine: SpeechEngineItem; onSaved: () => void }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [key, setKey] = useState("");
  const [region, setRegion] = useState("");
  const [busy, setBusy] = useState(false);
  const azure = engine.id === "azure-neural";
  return (
    <form
      className="k-inline"
      onSubmit={(e) => {
        e.preventDefault();
        setBusy(true);
        request(Method.voiceSetKey, { engine: engine.id, key, region: azure ? region : null })
          .then(() => {
            setKey("");
            toast(t("speech.cloud.saved", { name: engine.name }));
            onSaved();
          })
          .catch((err: unknown) => toast(message(err)))
          .finally(() => setBusy(false));
      }}
    >
      <TextField
        type="password"
        autoComplete="off"
        value={key}
        onChange={(e) => setKey(e.target.value)}
        placeholder={t("speech.cloud.keyPlaceholder")}
        aria-label={t("speech.cloud.key", { name: engine.name })}
      />
      {azure && (
        <TextField
          value={region}
          onChange={(e) => setRegion(e.target.value)}
          placeholder={t("speech.cloud.regionPlaceholder")}
          aria-label={t("speech.cloud.region")}
        />
      )}
      <Button type="submit" disabled={busy || !key.trim() || (azure && !region.trim())}>
        {busy ? t("speech.cloud.testing") : t("speech.cloud.testAndSave")}
      </Button>
    </form>
  );
}

export function CloudEngines({ slot, speech }: { slot: Slot; speech: Speech }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [open, setOpen] = useState<string | null>(null);
  const choices = speech.choices;
  if (!choices) return null;
  const current = slot === "stt" ? choices.stt : choices.tts;
  const cloud = choices.engines.filter((e) => e.slot === slot && e.privacy === "cloud");
  if (cloud.length === 0) return null;
  return (
    <details className="k-cloud-engines">
      <summary>{t("speech.cloud.title")}</summary>
      <p className="k-note">{t(`speech.cloud.hint.${slot}`)}</p>
      <Group>
        {cloud.map((e) => (
          <div key={e.id}>
            <Row
              lead={<Monogram text={monogram(e.name)} color={brainColor(e.id)} />}
              title={e.name}
              subtitle={
                e.fitsLanguage
                  ? t(`speech.cloud.sends.${slot}`)
                  : t("speech.notForLanguage", { language: choices.language })
              }
              end={
                <>
                  {e.id === current && <Tag tone="success">{t("speech.inUse")}</Tag>}
                  {e.ready ? (
                    <>
                      {e.id !== current && (
                        <Button
                          size="sm"
                          variant="primary"
                          disabled={!e.fitsLanguage}
                          onClick={() => void speech.choose(slot, e.id).catch((err: unknown) => toast(message(err)))}
                        >
                          {t("speech.cloud.use")}
                        </Button>
                      )}
                      <Button
                        size="sm"
                        variant="plain"
                        onClick={() =>
                          void request(Method.voiceDeleteKey, { engine: e.id })
                            .then(speech.reload)
                            .catch((err: unknown) => toast(message(err)))
                        }
                      >
                        {t("speech.cloud.removeKey")}
                      </Button>
                    </>
                  ) : (
                    <Button
                      size="sm"
                      onClick={() => setOpen((o) => (o === e.id ? null : e.id))}
                      aria-expanded={open === e.id}
                    >
                      {t("speech.cloud.addKey")}
                    </Button>
                  )}
                </>
              }
            />
            {open === e.id && !e.ready && (
              <div className="k-cloud-engines__key">
                <KeyForm
                  engine={e}
                  onSaved={() => {
                    setOpen(null);
                    speech.reload();
                  }}
                />
              </div>
            )}
          </div>
        ))}
      </Group>
    </details>
  );
}
