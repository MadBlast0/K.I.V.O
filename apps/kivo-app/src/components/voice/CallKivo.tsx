/**
 * How the user calls KIVO (UX §4): "Hey Kivo" (hands-free) and push-to-talk (hold the keys), each
 * with its own switch. At least one always stays on, so KIVO can always be reached: the switch
 * that would leave none is locked, with the reason beside it (the runtime refuses it too).
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Method, type CapabilityItem } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { Explain, Group, Note, Row, ShortcutRecorder, Switch, useToast } from "../ui";
import { enable, useWakeWords } from "./WakeWords";

const DEFAULT_KEYS = ["Ctrl", "Space"];
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export interface CallWays {
  /** "Hey Kivo" (or another wake word) can start KIVO now; `null` until known. */
  wake: boolean | null;
  /** Push-to-talk is on; `null` until known. */
  ptt: boolean | null;
  keys: string[];
  setWake: (on: boolean) => void;
  setPtt: (on: boolean) => void;
  setKeys: (keys: string[]) => void;
}

/** The ways to call KIVO, as the runtime has them. */
export function useCallWays(): CallWays {
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const { list, load } = useWakeWords();
  const [ptt, setPttState] = useState<boolean | null>(null);
  const [keys, setKeysState] = useState<string[]>(DEFAULT_KEYS);
  const loadPtt = useCallback(() => {
    if (!connected) return;
    void request<CapabilityItem[]>(Method.capabilitiesGet)
      .then((all) => setPttState(all.find((c) => c.capability === "push-to-talk")?.enabled ?? true))
      .catch(() => {});
    void request<{ voice: Record<string, unknown> }>(Method.settingsGet)
      .then((s) => {
        const v = s.voice["push-to-talk"];
        if (Array.isArray(v)) setKeysState(v.map(String));
      })
      .catch(() => {});
  }, [connected, request]);
  useEffect(loadPtt, [loadPtt]);

  const word = list?.words.find((w) => w.builtIn);
  const wake = list ? list.listening && list.modelInstalled && list.words.some((w) => w.enabled) : null;
  return {
    wake,
    ptt,
    keys,
    setWake: (on) => {
      if (!list || !word) return;
      // Off turns every word off: "Hey Kivo" off means KIVO doesn't listen hands-free at all.
      const words = on ? [word] : list.words.filter((w) => w.enabled);
      void Promise.all(words.map((w) => enable(request, list, w.id, on)))
        .catch((e: unknown) => toast(message(e)))
        .finally(load);
    },
    setPtt: (on) => {
      void request(Method.capabilitiesSet, { capability: "push-to-talk", on })
        .then(() => setPttState(on))
        .catch((e: unknown) => toast(message(e)));
    },
    setKeys: (next) => {
      void request(Method.settingsSet, { voice: { "push-to-talk": next } })
        .then(() => setKeysState(next))
        .catch((e: unknown) => toast(message(e)));
    },
  };
}

/** The two switches, as one group. */
export function CallKivo({ ways }: { ways: CallWays }) {
  const { t } = useTranslation();
  const { wake, ptt, keys } = ways;
  if (wake === null || ptt === null) return null;
  // The one that is the only way left can't be switched off.
  const wakeLocked = wake && !ptt;
  const pttLocked = ptt && !wake;
  return (
    <>
      <Group>
        <Row
          icon="wave"
          title={t("callKivo.wake")}
          subtitle={
            <>
              {t("callKivo.wakeHint")} <Explain tip={t("callKivo.wakeTip")}>{t("callKivo.wakeTerm")}</Explain>
            </>
          }
          end={<Switch label={t("callKivo.wake")} checked={wake} disabled={wakeLocked} onChange={ways.setWake} />}
        />
        <Row
          icon="keyboard"
          title={t("callKivo.ptt")}
          subtitle={t("callKivo.pttHint")}
          end={
            <>
              {ptt && <ShortcutRecorder value={keys} onChange={ways.setKeys} />}
              <Switch label={t("callKivo.ptt")} checked={ptt} disabled={pttLocked} onChange={ways.setPtt} />
            </>
          }
        />
      </Group>
      {(wakeLocked || pttLocked) && <Note>{t("callKivo.oneStays")}</Note>}
    </>
  );
}
