/**
 * Choosing how KIVO hears and speaks (VOICE §11, UX-60): 2–4 profile cards per slot, each backed by
 * a registry engine, with its privacy label, size, languages and KIVO's own measurement (or "Not
 * benchmarked by KIVO"). Picking one shows the licence first when it downloads, then switches
 * safely: the old engine keeps working until the new one passes its test. Recognizers can be tried
 * on a sample; voices have a Preview.
 */
import { useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { ProfileItem, SpeechEngineItem, VoiceItem } from "../../ipc/generated";
import {
  Alert,
  Button,
  Dialog,
  DialogClose,
  Group,
  IconButton,
  OptionCard,
  Pill,
  RadioGroup,
  Row,
  Section,
  Spinner,
  Tag,
  useToast,
} from "../ui";
import type { Slot, Speech } from "./useSpeech";

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** "es" → "Spanish", in the UI's language. */
function languageName(ui: string, code: string): string {
  try {
    return new Intl.DisplayNames([ui], { type: "language" }).of(code) ?? code;
  } catch {
    return code;
  }
}

/** What a card says about its engine, in one line. */
function useFacts() {
  const { t, i18n } = useTranslation();
  const mb = useMemo(
    () => new Intl.NumberFormat(i18n.language, { style: "unit", unit: "megabyte", maximumFractionDigits: 0 }),
    [i18n.language],
  );
  const pct = useMemo(
    () => new Intl.NumberFormat(i18n.language, { style: "percent", maximumFractionDigits: 1 }),
    [i18n.language],
  );
  return (engine: SpeechEngineItem): string[] => {
    const facts = [t(`speech.privacy.${engine.privacy}`)];
    if (engine.model === null) facts.push(t("speech.builtIn"));
    else if (engine.ready) facts.push(t("speech.onThisPc"));
    else facts.push(t("speech.download", { size: mb.format(engine.downloadMb) }));
    facts.push(
      engine.languages.includes("*")
        ? t("speech.anyLanguage")
        : engine.languages.length === 1
          ? languageName(i18n.language, engine.languages[0] ?? "")
          : t("speech.languages", { count: engine.languages.length }),
    );
    const m = engine.measured;
    if (m?.wordErrorRate != null) facts.push(t("speech.accuracy", { wer: pct.format(m.wordErrorRate) }));
    if (m?.realTimeFactor != null) facts.push(t("speech.speed", { rtf: m.realTimeFactor.toFixed(2) }));
    if (!m) facts.push(t("speech.notBenchmarked"));
    if (!engine.commercialUse) facts.push(t("speech.personalUse"));
    return facts;
  };
}

/** A switch in progress, or why it failed, under the card it is about. */
function Progress({ speech, slot, engine }: { speech: Speech; slot: Slot; engine: string }) {
  const { t } = useTranslation();
  const p = speech.progress[slot];
  if (!p || p.engine !== engine) return null;
  if (p.stage === "failed") {
    return (
      <Alert kind="warning" title={t("speech.failed")}>
        {p.message}
      </Alert>
    );
  }
  if (p.stage === "ready") return <p className="k-speech__status">{t("speech.ready")}</p>;
  const label =
    p.stage === "downloading" && p.percent !== null
      ? t("speech.stage.downloadingPercent", { percent: p.percent / 100 })
      : t(`speech.stage.${p.stage}`);
  return (
    <p className="k-speech__status" role="status">
      <Spinner label={label} />
      {label}
    </p>
  );
}

function Unavailable({ profile, language }: { profile: ProfileItem; language: string }) {
  const { t, i18n } = useTranslation();
  const name = languageName(i18n.language, language);
  return (
    <div className="k-option k-option--off" aria-disabled="true">
      <span className="k-option__body">
        <span className="k-option__title">{t(`speech.profile.${profile.profile}`)}</span>
        <span className="k-option__desc">
          {profile.otherLanguagesOnly ? t("speech.notForLanguage", { language: name }) : t("speech.notYet")}
        </span>
      </span>
    </div>
  );
}

/** The profile cards for one slot. */
export function SpeechChooser({ slot, speech, children }: { slot: Slot; speech: Speech; children?: ReactNode }) {
  const { t } = useTranslation();
  const toast = useToast();
  const facts = useFacts();
  const [offer, setOffer] = useState<SpeechEngineItem | null>(null);
  const [sample, setSample] = useState<{ engine: string; text: string | null } | null>(null);
  const { choices, recommendation } = speech;
  if (!choices) return null;

  const engines = new Map(choices.engines.map((e) => [e.id, e]));
  const current = slot === "stt" ? choices.stt : choices.tts;
  const recommended = slot === "stt" ? recommendation?.sttEngine : recommendation?.ttsEngine;
  const profiles = choices.profiles.filter((p) => p.slot === slot);

  const start = (engine: SpeechEngineItem) => {
    speech.choose(slot, engine.id).catch((e: unknown) => toast(message(e)));
  };
  const pick = (id: string) => {
    const engine = engines.get(id);
    if (!engine || id === current) return;
    if (!engine.ready) setOffer(engine);
    else start(engine);
  };
  const trySample = (engine: string) => {
    setSample({ engine, text: null });
    speech
      .trySample(engine)
      .then((r) => setSample({ engine, text: r.heard || t("speech.heardNothing") }))
      .catch((e: unknown) => {
        setSample(null);
        toast(message(e));
      });
  };
  const model = offer?.model ? speech.models.find((m) => m.id === offer.model) : undefined;

  return (
    <>
      <RadioGroup label={t(`speech.${slot}Title`)} value={current} onChange={pick}>
        {profiles.map((profile) => {
          const engine = profile.engine ? engines.get(profile.engine) : undefined;
          if (!engine) return <Unavailable key={profile.profile} profile={profile} language={choices.language} />;
          return (
            <div key={profile.profile} className="k-speech__card">
              <OptionCard
                value={engine.id}
                title={t(`speech.profile.${profile.profile}`)}
                badge={
                  <>
                    {engine.id === recommended && <Pill tone="accent">{t("speech.recommended")}</Pill>}
                    {engine.id === current && <Tag tone="success">{t("speech.inUse")}</Tag>}
                  </>
                }
                description={
                  <>
                    <span className="k-speech__engine">{engine.name}</span> · {t(`speech.hint.${profile.profile}`)}
                    <span className="k-speech__facts">{facts(engine).join(" · ")}</span>
                  </>
                }
              />
              {slot === "stt" && engine.ready && (
                <div className="k-speech__actions">
                  <Button
                    size="sm"
                    variant="plain"
                    icon="play"
                    onClick={() => trySample(engine.id)}
                    disabled={sample?.engine === engine.id && sample.text === null}
                  >
                    {t("speech.trySample")}
                  </Button>
                  {sample?.engine === engine.id && (
                    <span className="k-speech__sample" role="status">
                      {sample.text === null ? t("speech.listening") : t("speech.heard", { text: sample.text })}
                    </span>
                  )}
                </div>
              )}
              <Progress speech={speech} slot={slot} engine={engine.id} />
            </div>
          );
        })}
      </RadioGroup>
      {children}

      <Dialog
        open={offer !== null}
        onOpenChange={(open) => !open && setOffer(null)}
        title={offer ? t("speech.downloadTitle", { name: offer.name }) : ""}
        description={offer ? t("speech.downloadBody") : ""}
        footer={
          <>
            <DialogClose>
              <Button>{t("voice.cancel")}</Button>
            </DialogClose>
            <Button
              variant="primary"
              icon="download"
              onClick={() => {
                if (offer) start(offer);
                setOffer(null);
              }}
            >
              {t("speech.downloadAndUse")}
            </Button>
          </>
        }
      >
        {offer && (
          <div className="k-licence">
            {!offer.commercialUse && (
              <Alert kind="warning" title={t("speech.personalUseTitle")}>
                {t("speech.personalUseBody")}
              </Alert>
            )}
            <p>
              <b>{t("voice.license")}</b> {offer.license}
            </p>
            {model && <p>{model.attribution}</p>}
            {model && <p className="k-licence__source">{model.source}</p>}
          </div>
        )}
      </Dialog>
    </>
  );
}

/** The voices of the engine KIVO speaks with, each with a Preview (VOICE §11: the engine and the
 * voice are separate choices). */
export function VoiceList({ speech }: { speech: Speech }) {
  const { t, i18n } = useTranslation();
  const toast = useToast();
  const [playing, setPlaying] = useState<string | null>(null);
  const { choices } = speech;
  const engine = choices?.engines.find((e) => e.id === choices.tts);
  if (!choices || !engine || engine.voices.length === 0 || !engine.ready) return null;
  const language = (v: VoiceItem) =>
    v.languages.includes("*")
      ? t("speech.anyLanguage")
      : v.languages.map((l) => languageName(i18n.language, l)).join(", ");
  const selected = choices.ttsVoice || engine.voices[0]?.id;
  const preview = (voice: string) => {
    setPlaying(voice);
    speech
      .preview(engine.id, voice)
      .catch((e: unknown) => toast(message(e)))
      .finally(() => setPlaying(null));
  };
  return (
    <>
      <Section title={t("speech.voices")} aside={t("speech.voicesHint")} />
      <Group>
        {engine.voices.map((v) => (
          <Row
            key={v.id}
            lead={
              <IconButton
                icon="play"
                label={t("speech.preview", { name: v.name })}
                onClick={() => preview(v.id)}
                disabled={playing !== null}
              />
            }
            title={v.name}
            subtitle={[v.style && t(`speech.style.${v.style}`), language(v)].filter(Boolean).join(" · ")}
            end={
              v.id === selected ? (
                <Tag tone="success">{t("speech.inUse")}</Tag>
              ) : (
                <Button
                  size="sm"
                  onClick={() => {
                    speech.pickVoice(v.id).catch((e: unknown) => toast(message(e)));
                  }}
                >
                  {t("speech.use")}
                </Button>
              )
            }
          />
        ))}
      </Group>
    </>
  );
}

/** "Recommended for your PC" with its reason (UX-60). */
export function Recommended({ speech, onUse }: { speech: Speech; onUse?: () => void }) {
  const { t } = useTranslation();
  const toast = useToast();
  const { recommendation: r, choices } = speech;
  if (!r || !choices) return null;
  const name = (id: string | null | undefined) => choices.engines.find((e) => e.id === id)?.name ?? "";
  const use = async () => {
    try {
      if (r.sttEngine && r.sttEngine !== choices.stt) await speech.choose("stt", r.sttEngine);
      if (r.ttsEngine !== choices.tts) await speech.choose("tts", r.ttsEngine);
      onUse?.();
    } catch (e) {
      toast(message(e));
    }
  };
  return (
    <div className="k-tile k-speech__recommended">
      <div className="k-speech__recommended-title">{t("speech.recommendedTitle")}</div>
      <div>
        {r.sttEngine
          ? t("speech.recommendedPair", { stt: name(r.sttEngine), tts: name(r.ttsEngine) })
          : t("speech.recommendedNoStt", { tts: name(r.ttsEngine) })}
      </div>
      <div className="k-speech__reason">{r.reason}</div>
      <div className="k-speech__actions">
        <Button
          size="sm"
          variant="primary"
          onClick={() => {
            void use();
          }}
        >
          {t("speech.useRecommended")}
        </Button>
      </div>
    </div>
  );
}
