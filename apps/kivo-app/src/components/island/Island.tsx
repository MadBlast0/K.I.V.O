/**
 * The Island: KIVO's black pill at the top of the screen. It morphs between states with a
 * spring (width and height animate together). The waveform draws only while the Island is
 * audible and stops completely at rest, so an idle Island costs zero frames. When the state
 * changes, the new content cross-fades in 120 ms after the shape starts to morph.
 */
import { AnimatePresence, motion, useReducedMotionConfig } from "motion/react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Icon } from "../../icons";
import { Keys, Orb } from "../ui/Status";
import { useTranslation } from "react-i18next";
import { withNodes } from "../../i18n/nodes";

export interface IslandModel {
  /** Identifies the state; content cross-fades when it changes (e.g. "listening", "acting"). */
  state: string;
  /** Target width in px; height sizes to content. */
  width: number;
  label: ReactNode;
  /** Muted text after the label (“for a follow-up”, “Spotify”). */
  sub?: ReactNode;
  /** Leading glyph; defaults to the orb. */
  lead?: ReactNode;
  /** Trailing element (spinner, chip, keys, buttons). */
  trail?: ReactNode;
  /** Shows the live waveform at the end of the row (after `trail`). */
  wave?: boolean;
  /** Expanded content under the row. */
  body?: ReactNode;
  /** Waveform colour: listening is white, KIVO speaking is light blue. */
  voice?: "user" | "kivo";
}

export function Island({
  model,
  level,
  className,
  "aria-label": ariaLabel,
}: {
  model: IslandModel | null;
  /** The live audio level (0–1) for the waveform; without it the waveform shows a sample envelope
   * (the component gallery). */
  level?: () => number;
  className?: string;
  "aria-label"?: string;
}) {
  const reduce = useReducedMotionConfig() ?? false;
  const [height, setHeight] = useState(36);

  // Size to content: one observer per mounted Island, attached through a callback ref
  // because the content mounts and unmounts with AnimatePresence.
  const observer = useRef<ResizeObserver | null>(null);
  const inner = useCallback((el: HTMLDivElement | null) => {
    observer.current?.disconnect();
    if (!el) return;
    setHeight(el.offsetHeight);
    observer.current = new ResizeObserver(() => setHeight(el.offsetHeight));
    observer.current.observe(el);
  }, []);
  useEffect(() => () => observer.current?.disconnect(), []);

  const tall = height > 40;
  const spring = reduce ? { duration: 0 } : { type: "spring" as const, stiffness: 420, damping: 34, mass: 0.9 };

  return (
    <AnimatePresence>
      {model && (
        <motion.div
          key="island"
          role="status"
          aria-live="polite"
          aria-label={ariaLabel}
          className={["k-island", className].filter(Boolean).join(" ")}
          initial={reduce ? false : { opacity: 0, scale: 0.6, width: 36, height: 36 }}
          animate={{ opacity: 1, scale: 1, width: model.width, height, borderRadius: tall ? 24 : 20 }}
          exit={reduce ? { opacity: 0 } : { opacity: 0, scale: 0.6, width: 36 }}
          transition={spring}
          style={{ maxWidth: "100%" }}
        >
          <div ref={inner} style={{ width: model.width, maxWidth: "100%", position: "relative" }}>
            {/* popLayout takes the outgoing content out of flow, so the height measures the new state. */}
            <AnimatePresence initial={false} mode="popLayout">
              <motion.div
                key={model.state}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: reduce ? { duration: 0 } : { delay: 0.12, duration: 0.18 } }}
                exit={{ opacity: 0, transition: { duration: reduce ? 0 : 0.08 } }}
              >
                <div className="k-island__row">
                  {model.lead ?? <Orb />}
                  <span className="k-island__label">
                    {model.label}
                    {model.sub && <small>{model.sub}</small>}
                  </span>
                  {model.trail}
                  {model.wave && <Waveform active voice={model.voice ?? "user"} level={level} />}
                </div>
                {model.body && <div className="k-island__body">{model.body}</div>}
              </motion.div>
            </AnimatePresence>
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

/* Drawn in an 88×36 space and shown at 44×18 (half size), as in the design. */
const W = 88;
const H = 36;
const SCALE = 0.5;
/** A level meter needs no more than 30 frames a second. Tied to requestAnimationFrame it redrew at
 * the display's rate (165 Hz on high-refresh laptops), and every frame of a transparent window
 * costs WebView2 CPU and power (`kivo-bench overlay`). */
const FPS = 30;
/** Smoothing toward the target level per frame (0.12 per frame at 60 fps, the same speed). */
const EASE = 1 - (1 - 0.12) ** (60 / FPS);

/** Five rounded bars, redrawn at 30 fps only while `active`, then eased to rest and stopped. */
export function Waveform({
  active,
  voice = "user",
  level,
}: {
  active: boolean;
  voice?: "user" | "kivo";
  level?: () => number;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const reduce = useReducedMotionConfig() ?? false;

  useEffect(() => {
    const c = canvas.current;
    const ctx = c?.getContext("2d");
    if (!c || !ctx) return;
    const colour = voice === "kivo" ? "#9AD3FF" : "#fff";
    // The backing store matches the display's pixels, so bars stay sharp at 150%/200%.
    const px = Math.max(1, window.devicePixelRatio || 1) * SCALE;
    c.width = Math.round(W * px);
    c.height = Math.round(H * px);
    ctx.setTransform(px, 0, 0, px, 0, 0);
    const draw = (amp: number, t: number) => {
      ctx.clearRect(0, 0, W, H);
      ctx.fillStyle = colour;
      for (let i = 0; i < 5; i++) {
        const h = 5 + (H - 8) * Math.max(0.08, amp * (0.5 + 0.5 * Math.abs(Math.sin(t * 6 + i * 1.3))));
        ctx.beginPath();
        ctx.roundRect(6 + i * 17, (H - h) / 2, 7, h, 3.5);
        ctx.fill();
      }
    };
    if (reduce) {
      draw(active ? 0.5 : 0, 0);
      return;
    }

    let amp = 0,
      t = 0,
      timer = 0;
    const tick = () => {
      t += 1 / FPS;
      // Real input comes from the runtime's level meter; the fallback is a plausible speech envelope.
      const target = active
        ? level
          ? level()
          : 0.35 + 0.65 * Math.abs(Math.sin(t * 3.1) * Math.sin(t * 1.7 + 1) + 0.25 * Math.sin(t * 9))
        : 0;
      amp += (target - amp) * EASE;
      draw(amp, t);
      if (!active && amp < 0.003) return; // at rest: stop scheduling frames
      timer = window.setTimeout(tick, 1000 / FPS);
    };
    tick();
    return () => window.clearTimeout(timer);
  }, [active, voice, level, reduce]);

  return <canvas ref={canvas} style={{ width: W * SCALE, height: H * SCALE, flex: "none" }} aria-hidden />;
}

/* ───────── Building blocks for Island bodies ───────── */

export function IslandSpin() {
  const { t } = useTranslation();
  return <span className="k-island__spin" role="img" aria-label={t("ui.working")} />;
}
export function IslandOk() {
  const { t } = useTranslation();
  return <span className="k-island__ok" role="img" aria-label={t("ui.done")} />;
}
/** A coloured status dot; decoration only, since the label says the same in words (UX §10). */
export const IslandDot = ({ color }: { color: string }) => (
  <span className="k-island__dot" style={{ background: color }} aria-hidden />
);
export const IslandApp = ({ text, bg, fg = "#fff" }: { text: string; bg: string; fg?: string }) => (
  <span className="k-island__app" style={{ background: bg, color: fg }}>
    {text}
  </span>
);
export const IslandRing = ({ progress = 70 }: { progress?: number }) => (
  <span className="k-island__ring" style={{ ["--p" as string]: `${progress}%` }} />
);
/** A ring that empties over `seconds` (the follow-up window, UX-45). CSS runs it, so it costs no
 * script frames; with reduced motion it stays full. */
export const IslandCountdown = ({ seconds, label }: { seconds: number; label: string }) => (
  <span
    className="k-island__ring k-island__ring--countdown"
    role="img"
    aria-label={label}
    style={{ ["--dur" as string]: `${seconds}s` }}
  />
);
export const IslandProgress = ({ value }: { value: number }) => (
  <span className="k-island__progress">
    <span style={{ width: `${value}%` }} />
  </span>
);
export const IslandChip = ({ children, danger }: { children: ReactNode; danger?: boolean }) => (
  <span className={danger ? "k-island__chip k-island__chip--danger" : "k-island__chip"}>{children}</span>
);

export interface IslandAction {
  label: string;
  kind?: "primary" | "danger";
  onClick?: () => void;
}

/** Buttons plus the matching voice hint: every button's word can be spoken. */
export function IslandActions({
  actions,
  hint = true,
  extraHint,
}: {
  actions: IslandAction[];
  hint?: boolean;
  extraHint?: ReactNode;
}) {
  const words = actions.map((a) => a.label.toLowerCase());
  return (
    <>
      <div className="k-island__buttons">
        {actions.map((a) => (
          <button
            key={a.label}
            type="button"
            onClick={a.onClick}
            className={a.kind ? `k-island__btn k-island__btn--${a.kind}` : "k-island__btn"}
          >
            {a.label}
          </button>
        ))}
      </div>
      {hint && <VoiceHint words={words} extra={extraHint} />}
    </>
  );
}

/** “Say “allow once”, “allow for this project” or “deny””: the words joined the way the language
 * joins alternatives (`Intl.ListFormat`), each in bold. */
export function VoiceHint({ words, extra }: { words: string[]; extra?: ReactNode }) {
  const { t, i18n } = useTranslation();
  const parts = new Intl.ListFormat(i18n.language, { type: "disjunction" }).formatToParts(words);
  const list = parts.map((p, i) =>
    p.type === "element" ? <b key={i}>{t("island.quoted", { word: p.value })}</b> : <span key={i}>{p.value}</span>,
  );
  return (
    <div className="k-island__hint">
      <span className="k-island__mic" aria-hidden>
        <Icon name="mic" />
      </span>
      <span>
        {withNodes(t, "island.say", { words: list })}
        {extra}
      </span>
    </div>
  );
}

export function IslandRisk({ level }: { level: "medium" | "high" }) {
  const { t } = useTranslation();
  return (
    <div className={level === "high" ? "k-island__risk k-island__risk--high" : "k-island__risk"}>
      <Icon name={level === "high" ? "permissions" : "warning"} />
      {level === "high" ? t("island.riskHigh") : t("island.riskMedium")}
    </div>
  );
}

export const IslandKeys = ({ keys }: { keys: string[] }) => <Keys keys={keys} />;
