/**
 * Teaching KIVO the owner's voice (VOICE §5, VOICE-20/22, UX-33 step 5): consent first, then eight
 * short prompts recorded through KIVO's listener. Clips are encrypted on this PC; one click deletes
 * everything. Once enrolled, the speaker mode says who may use KIVO by voice.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Method } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { Alert, Button, Checkbox, Group, Row, Select, Tag, useToast } from "../ui";
import { LiveLevel, useMicLevel } from "./MicCheck";

interface Status {
  enrolled: boolean;
  prompts: string[];
  recorded: boolean[];
  embeddings: number;
}

interface Take {
  ok: boolean;
  seconds: number;
}

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function Enrollment({ onDone }: { onDone?: () => void }) {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [status, setStatus] = useState<Status | null>(null);
  const [consent, setConsent] = useState(false);
  const [started, setStarted] = useState(false);
  const [recording, setRecording] = useState(false);
  const [retry, setRetry] = useState(false);
  const level = useMicLevel(recording);

  const load = useCallback(() => {
    if (!connected) return;
    void request<Status>(Method.voiceIdStatus)
      .then(setStatus)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);

  if (!status) return null;
  const next = status.recorded.findIndex((r) => !r);
  const done = status.recorded.filter(Boolean).length;

  const start = () =>
    request<Status>(Method.voiceIdStart, { consent: true })
      .then((s) => {
        setStatus(s);
        setStarted(true);
      })
      .catch((e: unknown) => toast(message(e)));

  const record = (prompt: number) => {
    setRecording(true);
    setRetry(false);
    request<Take>(Method.voiceIdRecord, { prompt })
      .then((take) => {
        setRetry(!take.ok);
        load();
      })
      .catch((e: unknown) => toast(message(e)))
      .finally(() => setRecording(false));
  };

  const finish = () =>
    request<Status>(Method.voiceIdFinish)
      .then((s) => {
        setStatus(s);
        setStarted(false);
        onDone?.();
      })
      .catch((e: unknown) => toast(message(e)));

  const remove = () =>
    request<Status>(Method.voiceIdDelete)
      .then(setStatus)
      .catch((e: unknown) => toast(message(e)));

  if (status.enrolled && !started) {
    return (
      <Group>
        <Row
          icon="user"
          title={t("enroll.enrolled")}
          subtitle={t("enroll.enrolledHint")}
          end={<Tag tone="success">{t("enroll.on")}</Tag>}
        />
        <Row
          icon="refresh"
          title={t("enroll.again")}
          end={
            <Button size="sm" onClick={() => void start()}>
              {t("enroll.redo")}
            </Button>
          }
        />
        <Row
          icon="delete"
          title={t("enroll.delete")}
          subtitle={t("enroll.deleteHint")}
          end={
            <Button size="sm" variant="destructive" onClick={() => void remove()}>
              {t("enroll.deleteButton")}
            </Button>
          }
        />
      </Group>
    );
  }

  if (!started) {
    return (
      <div className="k-enroll">
        <Checkbox checked={consent} onChange={setConsent}>
          {t("enroll.consent")}
        </Checkbox>
        <Button variant="primary" icon="mic" disabled={!consent} onClick={() => void start()}>
          {t("enroll.start")}
        </Button>
      </div>
    );
  }

  const total = status.prompts.length;
  return (
    <div className="k-tile k-enroll__card">
      {next >= 0 ? (
        <>
          <div className="k-enroll__count">{t("enroll.progress", { n: next + 1, total })}</div>
          <div className="k-enroll__prompt">“{status.prompts[next]}”</div>
          <div className="k-enroll__record">
            <LiveLevel level={level} bars={7} />
            <button
              type="button"
              className={recording ? "k-recb k-recb--on" : "k-recb"}
              aria-label={recording ? t("enroll.recording") : t("enroll.record")}
              onClick={() => record(next)}
              disabled={recording}
            >
              <span aria-hidden>●</span>
            </button>
            <LiveLevel level={level} bars={7} />
          </div>
          <p className="k-enroll__hint" role="status">
            {recording ? t("enroll.speakNow") : retry ? t("enroll.retry") : t("enroll.pressToRecord")}
          </p>
        </>
      ) : (
        <Alert kind="success" title={t("enroll.allDone")}>
          {t("enroll.allDoneBody")}
        </Alert>
      )}
      <div className="k-enroll__dots" aria-label={t("enroll.recorded", { n: done, total })}>
        {status.recorded.map((r, i) => (
          <i key={i} className={r ? "is-done" : i === next ? "is-on" : undefined} />
        ))}
      </div>
      <div className="k-enroll__actions">
        <Button
          variant="plain"
          onClick={() => {
            void request(Method.voiceIdCancel).then(load);
            setStarted(false);
          }}
        >
          {t("voice.cancel")}
        </Button>
        <Button variant="primary" disabled={done < total} onClick={() => void finish()}>
          {t("enroll.finish")}
        </Button>
      </div>
    </div>
  );
}

/** Who may use KIVO by voice once it knows the owner's voice (VOICE-22). */
export function SpeakerMode() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const [mode, setMode] = useState<string | null>(null);
  const connected = link?.status === "connected";
  useEffect(() => {
    if (!connected) return;
    void request<{ voice: Record<string, unknown> }>(Method.settingsGet)
      .then((s) => {
        const saved = s.voice["speaker-mode"];
        setMode(typeof saved === "string" ? saved : "off");
      })
      .catch(() => {});
  }, [connected, request]);
  if (mode === null) return null;
  const choose = (value: string) =>
    request(Method.settingsSet, { voice: { "speaker-mode": value } })
      .then(() => setMode(value))
      .catch((e: unknown) => toast(message(e)));
  return (
    <Group>
      <Row
        icon="users"
        title={t("enroll.mode")}
        subtitle={t(`enroll.modeHint.${mode}`)}
        end={
          <Select
            label={t("enroll.mode")}
            value={mode}
            onChange={(v) => void choose(v)}
            items={["off", "prefer-owner", "owner-only"].map((value) => ({
              value,
              label: t(`enroll.modes.${value}`),
            }))}
          />
        }
      />
    </Group>
  );
}
