/**
 * The screen overlay's page. The wake glow (UX-16): a 3 px light along the screen's edges that
 * fades in and out in about 400 ms each time the runtime says KIVO woke; nothing runs between
 * glows, and with reduced motion the light shows briefly without fading. Computer use (CAP-12):
 * while KIVO controls the screen, the frame the user chose, a banner with how to stop it, and
 * KIVO's own cursor where it acts.
 */
import { listen } from "@tauri-apps/api/event";
import "./glow.css";

const glow = document.getElementById("glow");

/** Plays the glow once (restarting it if one is still playing). */
export function play(el: HTMLElement | null): void {
  if (!el) return;
  el.classList.remove("k-glow--on");
  // Reading layout restarts the CSS animation.
  void el.offsetWidth;
  el.classList.add("k-glow--on");
}

void listen("kivo://glow", () => play(glow));

/** What the runtime says about computer use; `null` when it ended. */
export interface ControlState {
  frame: "off" | "subtle" | "full";
  paused: boolean;
  app: string;
  step: number;
  maxSteps: number;
  cursor: { x: number; y: number } | null;
}

/** Shows computer use's frame, banner and cursor (or clears them). */
export function control(root: HTMLElement | null, state: ControlState | null, banner: string): void {
  if (!root) return;
  root.hidden = state === null;
  if (!state) return;
  root.dataset.frame = state.frame;
  root.dataset.paused = String(state.paused);
  const text = root.querySelector<HTMLElement>(".k-control__banner");
  if (text) text.textContent = banner;
  const cursor = root.querySelector<HTMLElement>(".k-control__cursor");
  if (cursor) {
    cursor.hidden = state.cursor === null;
    if (state.cursor) cursor.style.transform = `translate(${state.cursor.x}px, ${state.cursor.y}px)`;
  }
}

const BANNER = "KIVO is controlling your screen — Stop (Ctrl+Alt+Shift+Esc)";
void listen<ControlState | null>("kivo://control", (e) =>
  control(document.getElementById("control"), e.payload, BANNER),
);
