/**
 * The Character companion (UX-38): a small mascot whose face follows the same `SessionState` as
 * the Island — idle, listening, thinking, acting, speaking, asking, pointing, done and error. It
 * is drawn in SVG by KIVO itself (DECISIONS "M8 build": no Rive asset exists yet; a designed Rive
 * character can replace this drawing later without changing the states).
 *
 * Idle cost: nothing moves at rest. Blinks and the thinking glance are CSS animations present
 * only while KIVO is active, and the mouth and listening ring follow the audio level through one
 * animation frame loop that runs only while KIVO listens or speaks.
 */
import { useEffect, useId, useRef } from "react";

export type Mood = "idle" | "listening" | "thinking" | "acting" | "speaking" | "asking" | "pointing" | "done" | "error";

/** The face for an Island state (`listening`, `thinking`, `confirm-…`, `done-…`, `error`, …). */
export function moodOf(state: string, pointing = false): Mood {
  if (state.startsWith("error")) return "error";
  if (state.startsWith("confirm") || state.startsWith("draft") || state === "awaiting") return "asking";
  if (pointing) return "pointing";
  if (state === "listening" || state.startsWith("followUp")) return "listening";
  if (state === "thinking") return "thinking";
  if (state === "acting") return "acting";
  if (state === "speaking") return "speaking";
  if (state.startsWith("done")) return "done";
  return "idle";
}

/** Moods in which something moves; every other face is a still picture. */
const ACTIVE: ReadonlySet<Mood> = new Set(["listening", "thinking", "acting", "speaking", "pointing"]);
/** Moods that follow the audio level. */
const AUDIBLE: ReadonlySet<Mood> = new Set(["listening", "speaking"]);

export function Character({
  mood,
  level,
  label,
}: {
  mood: Mood;
  /** The live audio level (0–1). */
  level?: () => number;
  label: string;
}) {
  const ref = useRef<SVGSVGElement>(null);
  const body = `k-character-body-${useId().replace(/:/g, "")}`;
  const audible = AUDIBLE.has(mood) && level !== undefined;

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (!audible || !level) {
      el.style.setProperty("--level", "0");
      return;
    }
    let frame = 0;
    let smooth = 0;
    const tick = () => {
      // Quick to open, slower to close, like a mouth.
      const target = Math.min(1, Math.max(0, level()));
      smooth += (target - smooth) * (target > smooth ? 0.55 : 0.25);
      el.style.setProperty("--level", smooth.toFixed(3));
      frame = requestAnimationFrame(tick);
    };
    frame = requestAnimationFrame(tick);
    return () => cancelAnimationFrame(frame);
  }, [audible, level]);

  const active = ACTIVE.has(mood);
  return (
    <svg
      ref={ref}
      className={`k-character k-character--${mood}${active ? " k-character--active" : ""}`}
      viewBox="0 0 64 64"
      role="img"
      aria-label={label}
      data-mood={mood}
    >
      <defs>
        <radialGradient id={body} cx="40%" cy="30%" r="75%">
          <stop offset="0%" stopColor="#3a3a3f" />
          <stop offset="100%" stopColor="#111113" />
        </radialGradient>
      </defs>
      {/* The listening ring grows with the voice it hears. */}
      <circle className="k-character__ring" cx="32" cy="32" r="29" />
      <rect className="k-character__body" x="6" y="8" width="52" height="50" rx="22" fill={`url(#${body})`} />
      <g className="k-character__brows">
        <path className="k-character__brow k-character__brow--left" d="M17 21 Q22 18 27 20" />
        <path className="k-character__brow k-character__brow--right" d="M37 20 Q42 18 47 21" />
      </g>
      <g className="k-character__eyes">
        <g className="k-character__eye k-character__eye--left">
          <ellipse className="k-character__white" cx="22" cy="29" rx="5" ry="6" />
          <circle className="k-character__pupil" cx="22" cy="29.5" r="2.4" />
          <path className="k-character__closed" d="M17 30 Q22 25 27 30" />
        </g>
        <g className="k-character__eye k-character__eye--right">
          <ellipse className="k-character__white" cx="42" cy="29" rx="5" ry="6" />
          <circle className="k-character__pupil" cx="42" cy="29.5" r="2.4" />
          <path className="k-character__closed" d="M37 30 Q42 25 47 30" />
        </g>
      </g>
      <g className="k-character__mouth">
        <path className="k-character__smile" d="M25 43 Q32 48 39 43" />
        <path className="k-character__frown" d="M25 46 Q32 41 39 46" />
        <ellipse className="k-character__open" cx="32" cy="44" rx="5" ry="3.5" />
      </g>
      {/* Thinking: three dots that fill in turn. */}
      <g className="k-character__dots">
        <circle cx="46" cy="10" r="1.8" />
        <circle cx="51" cy="7" r="2.2" />
        <circle cx="57" cy="4" r="2.6" />
      </g>
    </svg>
  );
}
