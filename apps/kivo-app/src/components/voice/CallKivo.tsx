/**
 * How the user calls KIVO (UX §4): "Hey Kivo" (hands-free) and push-to-talk (hold the keys), each
 * with its own switch. At least one always stays on, so KIVO can always be reached: the switch
 * that would leave none is locked, with the reason beside it (the runtime refuses it too).
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Method, type CapabilityItem } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { cn } from "../../lib/cn";
import { Explain, Note, ShortcutRecorder, Switch, useToast } from "../ui";
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

/** The two ways, as two tiles side by side: what each is, its switch, and for push-to-talk its
 * keys (press them to change). */
export function CallKivo({ ways }: { ways: CallWays }) {
  const { t } = useTranslation();
  const { wake, ptt, keys } = ways;
  if (wake === null || ptt === null) return null;
  // The one that is the only way left can't be switched off.
  const wakeLocked = wake && !ptt;
  const pttLocked = ptt && !wake;
  return (
    <>
      <div className="k-ways">
        <section className={cn("k-way", wake && "is-on")} aria-label={t("callKivo.wake")}>
          <div className="k-way__art k-way__art--wave" aria-hidden>
            {[0, 1, 2, 3, 4, 5, 6].map((i) => (
              <i key={i} />
            ))}
          </div>
          <div className="k-way__head">
            <b>{t("callKivo.wake")}</b>
            <Switch label={t("callKivo.wake")} checked={wake} disabled={wakeLocked} onChange={ways.setWake} />
          </div>
          <p>{t("callKivo.wakeHint")}</p>
          <Explain tip={t("callKivo.wakeTip")}>{t("callKivo.wakeTerm")}</Explain>
        </section>
        <section className={cn("k-way", ptt && "is-on")} aria-label={t("callKivo.ptt")}>
          <div className="k-way__art k-way__art--keys">
            <ShortcutRecorder value={keys} onChange={ways.setKeys} />
          </div>
          <div className="k-way__head">
            <b>{t("callKivo.ptt")}</b>
            <Switch label={t("callKivo.ptt")} checked={ptt} disabled={pttLocked} onChange={ways.setPtt} />
          </div>
          <p>{t("callKivo.pttHint")}</p>
          <span className="k-way__foot">{t("callKivo.pttChange")}</span>
        </section>
      </div>
      {(wakeLocked || pttLocked) && <Note>{t("callKivo.oneStays")}</Note>}
    </>
  );
}
