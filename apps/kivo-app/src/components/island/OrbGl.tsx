/**
 * The Orb companion (UX-38): a soft, living sphere drawn by one small WebGL shader. Its edge
 * breathes with the voice level and its colour follows KIVO's state. It draws at 30 fps only
 * while KIVO is active; at rest it settles, draws one last frame and stops, so an idle Orb costs
 * zero frames, like the pill. Reduced motion draws a single still frame. Without WebGL (old
 * drivers, tests) the static CSS orb stands in.
 */
import { useReducedMotionConfig } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { Orb } from "../ui/Status";

const VERTEX = `
attribute vec2 a_pos;
void main() { gl_Position = vec4(a_pos, 0.0, 1.0); }
`;

// A blob whose edge is three slow sine waves around the circle, pushed out by the voice level,
// with a bright core and a faint halo. Premultiplied alpha over the transparent window.
const FRAGMENT = `
precision mediump float;
uniform vec2 u_res;
uniform float u_time;
uniform float u_level;
uniform vec3 u_color;
void main() {
  vec2 p = (gl_FragCoord.xy * 2.0 - u_res) / u_res.y;
  float r = length(p);
  float a = atan(p.y, p.x);
  float wobble = 0.05 * sin(a * 3.0 + u_time * 1.7)
               + 0.04 * sin(a * 5.0 - u_time * 2.3)
               + 0.03 * sin(a * 2.0 + u_time * 0.9);
  float edge = 0.6 + wobble * (0.5 + u_level * 1.8) + u_level * 0.12;
  float body = smoothstep(edge, edge - 0.2, r);
  float core = smoothstep(0.5, 0.0, r);
  float halo = smoothstep(edge + 0.35, edge, r) * (0.18 + u_level * 0.4);
  vec3 colour = mix(u_color * 0.6, vec3(1.0), core * 0.5);
  float alpha = max(body, halo);
  gl_FragColor = vec4(colour * alpha, alpha);
}
`;

/** Plenty for a breathing orb; every frame of a transparent window costs power. */
const FPS = 30;
const EASE = 1 - (1 - 0.12) ** (60 / FPS);

function rgb(hex: string): [number, number, number] {
  const n = Number.parseInt(hex.replace("#", ""), 16);
  return [((n >> 16) & 255) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
}

function program(gl: WebGLRenderingContext): WebGLProgram | null {
  const shader = (type: number, source: string) => {
    const s = gl.createShader(type);
    if (!s) return null;
    gl.shaderSource(s, source);
    gl.compileShader(s);
    return gl.getShaderParameter(s, gl.COMPILE_STATUS) ? s : null;
  };
  const vs = shader(gl.VERTEX_SHADER, VERTEX);
  const fs = shader(gl.FRAGMENT_SHADER, FRAGMENT);
  const p = gl.createProgram();
  if (!vs || !fs || !p) return null;
  gl.attachShader(p, vs);
  gl.attachShader(p, fs);
  gl.linkProgram(p);
  return gl.getProgramParameter(p, gl.LINK_STATUS) ? p : null;
}

export function OrbGl({
  color,
  active,
  level,
  size = 56,
  label,
}: {
  /** The state's colour (`#9AD3FF` while KIVO speaks, …). */
  color: string;
  /** KIVO is listening, thinking, acting or speaking: the orb moves. */
  active: boolean;
  level?: () => number;
  size?: number;
  label: string;
}) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const reduce = useReducedMotionConfig() ?? false;
  const [fallback, setFallback] = useState(false);

  useEffect(() => {
    const c = canvas.current;
    if (!c) return;
    const gl = c.getContext("webgl", { alpha: true, premultipliedAlpha: true, antialias: true });
    const prog = gl ? program(gl) : null;
    if (!gl || !prog) {
      setFallback(true);
      return;
    }
    const px = Math.max(1, window.devicePixelRatio || 1);
    c.width = Math.round(size * px);
    c.height = Math.round(size * px);
    gl.viewport(0, 0, c.width, c.height);
    // WebGL's `useProgram` only shares a React hook's prefix.
    // eslint-disable-next-line react/rules-of-hooks
    gl.useProgram(prog);
    const quad = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, quad);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([-1, -1, 1, -1, -1, 1, 1, 1]), gl.STATIC_DRAW);
    const pos = gl.getAttribLocation(prog, "a_pos");
    gl.enableVertexAttribArray(pos);
    gl.vertexAttribPointer(pos, 2, gl.FLOAT, false, 0, 0);
    const u = (name: string) => gl.getUniformLocation(prog, name);
    const [uRes, uTime, uLevel, uColor] = [u("u_res"), u("u_time"), u("u_level"), u("u_color")];
    gl.uniform2f(uRes, c.width, c.height);
    gl.uniform3f(uColor, ...rgb(color));
    gl.clearColor(0, 0, 0, 0);
    const draw = (time: number, amp: number) => {
      gl.uniform1f(uTime, time);
      gl.uniform1f(uLevel, amp);
      gl.clear(gl.COLOR_BUFFER_BIT);
      gl.drawArrays(gl.TRIANGLE_STRIP, 0, 4);
    };
    let timer = 0;
    // The program and buffer go with the effect: a state change builds them again.
    const release = () => {
      window.clearTimeout(timer);
      gl.deleteBuffer(quad);
      gl.deleteProgram(prog);
    };
    if (reduce) {
      draw(0, active ? 0.3 : 0);
      return release;
    }
    let amp = 0;
    let time = 0;
    const tick = () => {
      time += 1 / FPS;
      const target = active ? (level ? level() : 0.25 + 0.2 * Math.sin(time * 2)) : 0;
      amp += (target - amp) * EASE;
      draw(time, amp);
      // At rest: one settled frame, then no more.
      if (!active && amp < 0.003) return;
      timer = window.setTimeout(tick, 1000 / FPS);
    };
    tick();
    return release;
  }, [color, active, level, reduce, size]);

  if (fallback) return <Orb size={Math.round(size * 0.45)} />;
  return (
    <canvas ref={canvas} className="k-orb-gl" style={{ width: size, height: size }} role="img" aria-label={label} />
  );
}

/** The Orb's colour for an Island state (UX-38): the same palette as the pill's dots. */
export function orbColor(state: string, voice?: "user" | "kivo"): string {
  if (state.startsWith("error")) return "#FF5147";
  if (state.startsWith("confirm") || state.startsWith("draft") || state === "awaiting") return "#FFC857";
  if (state === "acting") return "#FFC857";
  if (state === "thinking") return "#B69CFF";
  if (voice === "kivo" || state === "speaking") return "#9AD3FF";
  if (state === "listening" || state.startsWith("followUp")) return "#FFFFFF";
  return "#8AB8FF";
}
