/**
 * The Island: KIVO's black pill at the top of the screen. It morphs between states with a
 * spring (width and height animate together). The waveform draws only while the Island is
 * audible and stops completely at rest, so an idle Island costs zero frames.
 */
import { AnimatePresence, motion, useReducedMotion } from "motion/react";
import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { Icon } from "../../icons";
import { Keys, Orb } from "../ui/Status";

export interface IslandModel {
  /** Target width in px; height sizes to content. */
  width: number;
  label: ReactNode;
  /** Muted text after the label (“for a follow-up”, “Spotify”). */
  sub?: ReactNode;
  /** Leading glyph; defaults to the orb. */
  lead?: ReactNode;
  /** Trailing element, or "wave" for the live waveform. */
  trail?: ReactNode | "wave";
  /** Expanded content under the row. */
  body?: ReactNode;
  /** Waveform colour: listening is white, KIVO speaking is light blue. */
  voice?: "user" | "kivo";
}

export function Island({ model, className, "aria-label": ariaLabel }: { model: IslandModel | null; className?: string; "aria-label"?: string }) {
  const reduce = useReducedMotion();
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
          <div ref={inner} style={{ width: model.width, maxWidth: "100%" }}>
            <div className="k-island__row">
              {model.lead ?? <Orb />}
              <span className="k-island__label">{model.label}{model.sub && <small>{model.sub}</small>}</span>
              {model.trail === "wave" ? <Waveform active voice={model.voice ?? "user"} /> : model.trail}
            </div>
            {model.body && <div className="k-island__body">{model.body}</div>}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

const W = 88;
const H = 36;

/** Five rounded bars. Runs requestAnimationFrame only while `active`, then eases to rest and stops. */
export function Waveform({ active, voice = "user", level }: { active: boolean; voice?: "user" | "kivo"; level?: () => number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const reduce = useReducedMotion();

  useEffect(() => {
    const c = canvas.current;
    const ctx = c?.getContext("2d");
    if (!c || !ctx) return;
    const colour = voice === "kivo" ? "#9AD3FF" : "#fff";
    // Draw in CSS pixels on a backing store scaled to the display, so bars stay sharp at 150%/200%.
    const dpr = Math.max(1, window.devicePixelRatio || 1);
    c.width = W * dpr;
    c.height = H * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
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
    if (reduce) { draw(active ? 0.5 : 0, 0); return; }

    let amp = 0, t = 0, raf = 0;
    const tick = () => {
      t += 1 / 60;
      // Real input comes from the runtime's level meter; the fallback is a plausible speech envelope.
      const target = active ? (level ? level() : 0.35 + 0.65 * Math.abs(Math.sin(t * 3.1) * Math.sin(t * 1.7 + 1) + 0.25 * Math.sin(t * 9))) : 0;
      amp += (target - amp) * 0.12;
      draw(amp, t);
      if (!active && amp < 0.003) return; // at rest: stop scheduling frames
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(raf);
  }, [active, voice, level, reduce]);

  return <canvas ref={canvas} style={{ width: W, height: H, flex: "none" }} aria-hidden />;
}

/* ───────── Building blocks for Island bodies ───────── */

export const IslandSpin = () => <span className="k-island__spin" aria-label="Working" />;
export const IslandOk = () => <span className="k-island__ok" aria-label="Done" />;
export const IslandDot = ({ color }: { color: string }) => <span className="k-island__dot" style={{ background: color }} />;
export const IslandApp = ({ text, bg, fg = "#fff" }: { text: string; bg: string; fg?: string }) => (
  <span className="k-island__app" style={{ background: bg, color: fg }}>{text}</span>
);
export const IslandRing = ({ progress = 70 }: { progress?: number }) => (
  <span className="k-island__ring" style={{ ["--p" as string]: `${progress}%` }} />
);
export const IslandProgress = ({ value }: { value: number }) => (
  <span className="k-island__progress"><span style={{ width: `${value}%` }} /></span>
);
export const IslandChip = ({ children, danger }: { children: ReactNode; danger?: boolean }) => (
  <span className={danger ? "k-island__chip k-island__chip--danger" : "k-island__chip"}>{children}</span>
);

export interface IslandAction { label: string; kind?: "primary" | "danger"; onClick?: () => void }

/** Buttons plus the matching voice hint: every button's word can be spoken. */
export function IslandActions({ actions, hint = true, extraHint }: { actions: IslandAction[]; hint?: boolean; extraHint?: ReactNode }) {
  const words = actions.map((a) => a.label.toLowerCase());
  return (
    <>
      <div className="k-island__buttons">
        {actions.map((a) => (
          <button key={a.label} type="button" onClick={a.onClick}
            className={a.kind ? `k-island__btn k-island__btn--${a.kind}` : "k-island__btn"}>{a.label}</button>
        ))}
      </div>
      {hint && <VoiceHint words={words} extra={extraHint} />}
    </>
  );
}

export function VoiceHint({ words, extra }: { words: string[]; extra?: ReactNode }) {
  return (
    <div className="k-island__hint">
      <span className="k-island__mic"><Icon name="mic" /></span>
      <span>Say {words.map((w, i) => (
        <span key={w}><b>“{w}”</b>{i < words.length - 2 ? ", " : i === words.length - 2 ? " or " : ""}</span>
      ))}{extra}</span>
    </div>
  );
}

export function IslandRisk({ level }: { level: "medium" | "high" }) {
  return (
    <div className={level === "high" ? "k-island__risk k-island__risk--high" : "k-island__risk"}>
      <Icon name={level === "high" ? "permissions" : "warning"} />{level === "high" ? "HIGH RISK" : "MEDIUM RISK"}
    </div>
  );
}

export const IslandKeys = ({ keys }: { keys: string[] }) => <Keys keys={keys} />;
