/**
 * Wake words (VOICE §4, VOICE-15/16/18): "Hey Kivo" plus up to four of the user's own. A new word is
 * checked (Good / Fair / Risky, with the reasons), heard in KIVO's voice and tried out loud before
 * it is saved; then samples of the user saying it tune its sensitivity, and a false-alarm test runs
 * it against minutes of ordinary speech. Everything runs on this PC.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Method } from "../../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../../ipc/runtime";
import {
  Alert,
  Button,
  Dialog,
  DialogClose,
  Group,
  IconButton,
  Row,
  Slider,
  Spinner,
  Switch,
  Tag,
  TextField,
  useToast,
  type Tone,
} from "../ui";
import { LiveLevel, useMicLevel } from "./MicCheck";

type Quality = "good" | "fair" | "risky";

export interface WakeWord {
  id: string;
  phrase: string;
  phonetic: string | null;
  enabled: boolean;
  builtIn: boolean;
  sensitivity: number;
  samples: string[];
  quality: Quality;
  falseAlarmTest: { minutes: number; falseAlarms: number } | null;
}

interface WakeList {
  words: WakeWord[];
  modelInstalled: boolean;
  listening: boolean;
}

interface Concern {
  reason: string;
  phrase?: string;
  phonemes?: number;
}

interface Assessment {
  quality: Quality;
  syllables: number;
  phonemes: number;
  concerns: Concern[];
}

interface Tried {
  heard: boolean;
  score: number | null;
  seconds: number;
}

/** The keyword model "Hey Kivo" and custom words need (DIST-12). */
export const KEYWORD_MODEL = "kws-zipformer-gigaspeech-en";
/** At most this many words listen at once (VOICE-18). */
const MAX_ENABLED = 5;

const TONE: Record<Quality, Tone> = { good: "success", fair: "warning", risky: "danger" };
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

function useWakeWords() {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [list, setList] = useState<WakeList | null>(null);
  const load = useCallback(() => {
    if (!connected) return;
    void request<WakeList>(Method.wakeList)
      .then(setList)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  useRuntimeEvents((event) => {
    if (event.group === "system" && event.event.type === "modelChanged" && event.event.id === KEYWORD_MODEL) load();
  });
  return { list, load, request };
}

/** Turns a word on: the keyword model downloads if it isn't here, and hands-free listening turns
 * on (the Microphone listening capability). */
async function enable(request: <T>(m: Method, p?: unknown) => Promise<T>, list: WakeList, id: string, on: boolean) {
  if (on && !list.modelInstalled) await request(Method.modelsInstall, { id: KEYWORD_MODEL });
  await request(Method.wakeSet, { id, enabled: on });
  if (on && !list.listening) await request(Method.capabilitiesSet, { capability: "mic-listening", on: true });
}

/** "Hey Kivo" on or off (onboarding step 3). */
export function HeyKivoSwitch() {
  const { t } = useTranslation();
  const toast = useToast();
  const { list, load, request } = useWakeWords();
  const word = list?.words.find((w) => w.builtIn);
  if (!list || !word) return null;
  const on = word.enabled && list.listening;
  return (
    <Switch
      label={t("wake.heyKivo")}
      checked={on}
      onChange={(v) => {
        enable(request, list, word.id, v)
          .catch((e: unknown) => toast(message(e)))
          .finally(load);
      }}
    />
  );
}

function concernText(t: (k: string, o?: Record<string, unknown>) => string, c: Concern) {
  return t(`wake.concern.${c.reason}`, { phrase: c.phrase ?? "", count: c.phonemes ?? 0 });
}

/** Adding a word: type → check → hear it → try it → save (VOICE-16). */
function AddWord({ onSaved, onClose }: { onSaved: (w: WakeWord) => void; onClose: () => void }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [phrase, setPhrase] = useState("");
  const [assessment, setAssessment] = useState<Assessment | null>(null);
  const [tried, setTried] = useState<Tried | null>(null);
  const [busy, setBusy] = useState<"check" | "hear" | "try" | "save" | null>(null);
  const level = useMicLevel(busy === "try");

  const run = <T,>(what: typeof busy, p: Promise<T>) => {
    setBusy(what);
    return p
      .catch((e: unknown) => {
        toast(message(e));
        return null;
      })
      .finally(() => setBusy(null));
  };
  const check = () =>
    void run("check", request<Assessment>(Method.wakeCheck, { phrase })).then((a) => {
      setAssessment(a);
      setTried(null);
    });
  const hear = () => void run("hear", request(Method.wakeHear, { phrase }));
  const tryIt = () => void run("try", request<Tried>(Method.wakeTry, { phrase })).then(setTried);
  const save = () =>
    void run("save", request<WakeWord>(Method.wakeSave, { phrase })).then((w) => {
      if (w) onSaved(w);
    });

  return (
    <Dialog
      open
      onOpenChange={(open) => !open && onClose()}
      title={t("wake.addTitle")}
      description={t("wake.addHint")}
      footer={
        <>
          <DialogClose>
            <Button>{t("voice.cancel")}</Button>
          </DialogClose>
          <Button
            variant="primary"
            disabled={!assessment || assessment.quality === "risky" || busy !== null}
            onClick={save}
          >
            {t("wake.save")}
          </Button>
        </>
      }
    >
      <div className="k-wake__add">
        <div className="k-wake__phrase">
          <TextField
            value={phrase}
            onChange={(e) => {
              setPhrase(e.target.value);
              setAssessment(null);
            }}
            placeholder={t("wake.placeholder")}
            aria-label={t("wake.phrase")}
          />
          <Button onClick={check} disabled={!phrase.trim() || busy !== null}>
            {t("wake.check")}
          </Button>
        </div>
        {assessment && (
          <>
            <div className="k-wake__quality">
              <Tag tone={TONE[assessment.quality]}>{t(`wake.quality.${assessment.quality}`)}</Tag>
              <span>{t(`wake.qualityHint.${assessment.quality}`)}</span>
            </div>
            {assessment.concerns.length > 0 && (
              <ul className="k-wake__concerns">
                {assessment.concerns.map((c, i) => (
                  <li key={i}>{concernText(t, c)}</li>
                ))}
              </ul>
            )}
            <div className="k-speech__actions">
              <Button size="sm" icon="volume" onClick={hear} disabled={busy !== null}>
                {t("wake.hear")}
              </Button>
              <Button size="sm" icon="mic" onClick={tryIt} disabled={busy !== null}>
                {t("wake.try")}
              </Button>
              {busy === "try" && <LiveLevel level={level} bars={9} />}
              {busy && busy !== "try" && <Spinner label={t("ui.working")} />}
            </div>
            {tried && (
              <p className="k-speech__status" role="status">
                {tried.heard ? t("wake.tryHeard") : t("wake.tryMissed")}
              </p>
            )}
          </>
        )}
      </div>
    </Dialog>
  );
}

/** One word's details: sensitivity, samples, tuning, the false-alarm test and delete. */
function EditWord({ word, onChange, onClose }: { word: WakeWord; onChange: () => void; onClose: () => void }) {
  const { t, i18n } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [current, setCurrent] = useState(word);
  const [busy, setBusy] = useState<"sample" | "tune" | "test" | null>(null);
  const [last, setLast] = useState<Tried | null>(null);
  const level = useMicLevel(busy === "sample");
  const number = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 });

  const refresh = (w: WakeWord | null) => {
    if (w) setCurrent(w);
    onChange();
  };
  const sample = () => {
    setBusy("sample");
    request<Tried>(Method.wakeSample, { id: current.id })
      .then((r) => {
        setLast(r);
        return request<WakeList>(Method.wakeList);
      })
      .then((l) => refresh(l.words.find((w) => w.id === current.id) ?? null))
      .catch((e: unknown) => toast(message(e)))
      .finally(() => setBusy(null));
  };
  const tune = () => {
    setBusy("tune");
    request<WakeWord>(Method.wakeTune, { id: current.id })
      .then(refresh)
      .catch((e: unknown) => toast(message(e)))
      .finally(() => setBusy(null));
  };
  const test = () => {
    setBusy("test");
    request<WakeWord["falseAlarmTest"]>(Method.wakeFalseAlarms, { id: current.id })
      .then((r) => refresh({ ...current, falseAlarmTest: r }))
      .catch((e: unknown) => toast(message(e)))
      .finally(() => setBusy(null));
  };
  const sensitivity = (value: number) => {
    const s = value / 100;
    setCurrent({ ...current, sensitivity: s });
    request(Method.wakeSet, { id: current.id, sensitivity: s })
      .then(onChange)
      .catch((e: unknown) => toast(message(e)));
  };
  const remove = () =>
    request(Method.wakeDelete, { id: current.id })
      .then(() => {
        onChange();
        onClose();
      })
      .catch((e: unknown) => toast(message(e)));

  const fa = current.falseAlarmTest;
  return (
    <Dialog
      open
      onOpenChange={(open) => !open && onClose()}
      title={`“${current.phrase}”`}
      description={t(`wake.qualityHint.${current.quality}`)}
      footer={
        <>
          {!current.builtIn && (
            <Button variant="destructive" icon="delete" onClick={() => void remove()}>
              {t("wake.delete")}
            </Button>
          )}
          <DialogClose>
            <Button variant="primary">{t("wake.done")}</Button>
          </DialogClose>
        </>
      }
    >
      <div className="k-wake__edit">
        <label className="k-wake__slider">
          <span>{t("wake.sensitivity")}</span>
          <Slider label={t("wake.sensitivity")} value={Math.round(current.sensitivity * 100)} onChange={sensitivity} />
          <span className="k-wake__scale">
            <span>{t("wake.fewerFalse")}</span>
            <span>{t("wake.wakesEasily")}</span>
          </span>
        </label>
        <Group>
          <Row
            icon="mic"
            title={t("wake.samples", { count: current.samples.length })}
            subtitle={
              busy === "sample"
                ? t("wake.sayIt", { phrase: current.phrase })
                : last
                  ? last.heard
                    ? t("wake.sampleKept")
                    : t("wake.sampleMissed")
                  : t("wake.samplesHint")
            }
            end={
              busy === "sample" ? (
                <LiveLevel level={level} bars={9} />
              ) : (
                <Button size="sm" onClick={sample} disabled={busy !== null}>
                  {t("wake.record")}
                </Button>
              )
            }
          />
          <Row
            icon="speed"
            title={t("wake.tune")}
            subtitle={t("wake.tuneHint")}
            end={
              <Button size="sm" onClick={tune} disabled={busy !== null || current.samples.length === 0}>
                {busy === "tune" ? <Spinner label={t("ui.working")} /> : t("wake.tuneButton")}
              </Button>
            }
          />
          <Row
            icon="permissions"
            title={t("wake.falseAlarms")}
            subtitle={
              fa
                ? t("wake.falseAlarmsResult", { count: fa.falseAlarms, minutes: number.format(fa.minutes) })
                : t("wake.falseAlarmsHint")
            }
            end={
              <Button size="sm" onClick={test} disabled={busy !== null}>
                {busy === "test" ? <Spinner label={t("ui.working")} /> : t("wake.runTest")}
              </Button>
            }
          />
        </Group>
      </div>
    </Dialog>
  );
}

/** The Voice page's list of wake words. */
export function WakeWords() {
  const { t } = useTranslation();
  const toast = useToast();
  const { list, load, request } = useWakeWords();
  const [adding, setAdding] = useState(false);
  const [editing, setEditing] = useState<WakeWord | null>(null);
  if (!list) return null;
  const enabled = list.words.filter((w) => w.enabled).length;

  return (
    <>
      {!list.modelInstalled && (
        <Alert kind="info" title={t("wake.noModelTitle")}>
          {t("wake.noModelBody")}
        </Alert>
      )}
      <Group>
        {list.words.map((w) => (
          <Row
            key={w.id}
            icon={w.builtIn ? "voice" : "mic"}
            title={`“${w.phrase}”`}
            subtitle={[
              w.builtIn ? t("wake.builtIn") : t("wake.custom"),
              t(`wake.quality.${w.quality}`),
              w.samples.length > 0 ? t("wake.samples", { count: w.samples.length }) : null,
            ]
              .filter(Boolean)
              .join(" · ")}
            end={
              <span className="k-inline">
                <IconButton icon="edit" label={t("wake.edit", { phrase: w.phrase })} onClick={() => setEditing(w)} />
                <Switch
                  label={t("wake.enabledFor", { phrase: w.phrase })}
                  checked={w.enabled && list.listening}
                  disabled={!w.enabled && enabled >= MAX_ENABLED}
                  onChange={(on) => {
                    enable(request, list, w.id, on)
                      .catch((e: unknown) => toast(message(e)))
                      .finally(load);
                  }}
                />
              </span>
            }
          />
        ))}
        <Row
          icon="add"
          title={t("wake.add")}
          subtitle={enabled >= MAX_ENABLED ? t("wake.max", { count: MAX_ENABLED }) : t("wake.addRowHint")}
          onClick={() => setAdding(true)}
        />
      </Group>
      {adding && (
        <AddWord
          onClose={() => setAdding(false)}
          onSaved={(w) => {
            setAdding(false);
            load();
            setEditing(w);
          }}
        />
      )}
      {editing && <EditWord word={editing} onChange={load} onClose={() => setEditing(null)} />}
    </>
  );
}
