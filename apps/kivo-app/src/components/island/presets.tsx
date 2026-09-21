/**
 * Sample model for every Island state. Used by the gallery and as a reference for how the
 * runtime's events map onto Island content.
 */
import type { IslandModel } from "./Island";
import {
  IslandActions,
  IslandApp,
  IslandChip,
  IslandDot,
  IslandKeys,
  IslandOk,
  IslandProgress,
  IslandRing,
  IslandRisk,
  IslandSpin,
} from "./Island";
import { Icon } from "../../icons";

export type IslandState =
  | "listening"
  | "followup"
  | "thinking"
  | "acting"
  | "speaking"
  | "undo"
  | "help"
  | "plan"
  | "confirm"
  | "high"
  | "draft"
  | "waiting"
  | "error"
  | "typing"
  | "guest"
  | "paused"
  | "media"
  | "timer"
  | "download"
  | "agent"
  | "cu"
  | "idle"
  | "bypass"
  | "capability-off"
  | "in-call"
  | "offline";

export const ISLAND_STATES: Array<{ id: IslandState; label: string }> = [
  { id: "listening", label: "Listening" },
  { id: "followup", label: "Follow-up" },
  { id: "thinking", label: "Thinking" },
  { id: "acting", label: "Acting" },
  { id: "speaking", label: "Speaking" },
  { id: "undo", label: "Undo" },
  { id: "help", label: "What can I say?" },
  { id: "plan", label: "Plan approval" },
  { id: "confirm", label: "Confirm" },
  { id: "high", label: "High risk" },
  { id: "draft", label: "Prompt draft" },
  { id: "waiting", label: "Waiting for you" },
  { id: "error", label: "Error" },
  { id: "typing", label: "Typing" },
  { id: "guest", label: "Guest" },
  { id: "paused", label: "Paused" },
];

/** Short notices the Island shows on its own (mockup → System surfaces → Island notices). */
export const ISLAND_NOTICES: Array<{ id: IslandState; label: string }> = [
  { id: "bypass", label: "Bypass is on" },
  { id: "capability-off", label: "Capability off" },
  { id: "in-call", label: "In a call" },
  { id: "offline", label: "Offline" },
];

export const ISLAND_LIVE: Array<{ id: IslandState; label: string }> = [
  { id: "media", label: "Music" },
  { id: "timer", label: "Timer" },
  { id: "download", label: "Download" },
  { id: "agent", label: "Agent task" },
  { id: "cu", label: "Computer use" },
  { id: "idle", label: "Hidden" },
];

const REQ = "Play something calm on Spotify and remind me at five to call Maya";
const ANS = "Playing Lo-fi Focus on Spotify. I’ll remind you to call Maya at 5 PM.";

export function islandPreset(
  state: IslandState,
  opts: { partial?: string; mode?: "ask" | "edits" | "plan" | "auto" | "bypass" } = {},
): IslandModel | null {
  const mode = opts.mode ?? "auto";
  const modeChip =
    mode === "bypass" ? (
      <IslandChip danger>BYPASS</IslandChip>
    ) : mode === "auto" ? null : (
      <IslandChip>{{ ask: "ASK", edits: "EDITS", plan: "PLAN" }[mode]}</IslandChip>
    );
  const p = opts.partial ?? "";

  switch (state) {
    case "listening":
      return { state, width: p ? 470 : 236, label: "Listening", wave: true, body: p ? <div>{p}</div> : undefined };
    case "followup":
      return { state, width: 300, label: "Listening", sub: "for a follow-up", trail: <IslandRing /> };
    case "thinking":
      return { state, width: 196, label: "Thinking", trail: <IslandSpin /> };
    case "acting":
      return {
        state,
        width: 480,
        label: "Working",
        sub: "2 steps",
        lead: <IslandApp text="S" bg="#1DB954" fg="#000" />,
        trail: (
          <>
            {modeChip}
            <IslandSpin />
          </>
        ),
        body: (
          <>
            <div className="k-island__quote">{REQ}</div>
            <div className="k-island__steps">
              <div>
                <IslandOk />
                <b>Spotify</b> Playing “Lo-fi Focus”
              </div>
              <div>
                <IslandSpin />
                <b>Reminders</b> Call Maya · 5:00 PM
              </div>
            </div>
          </>
        ),
      };
    case "speaking":
      return {
        state,
        width: 480,
        label: "KIVO",
        wave: true,
        voice: "kivo",
        body: (
          <>
            <div className="k-island__quote">{REQ}</div>
            <div className="k-island__answer">{ANS}</div>
          </>
        ),
      };
    case "undo":
      return {
        state,
        width: 400,
        label: "Moved 3 files to Archive",
        lead: <IslandApp text="▣" bg="#3B82F6" />,
        trail: (
          <>
            <button type="button" className="k-island__btn k-island__btn--primary">
              Undo
            </button>
            <IslandRing />
          </>
        ),
      };
    case "help":
      return {
        state,
        width: 480,
        label: "What you can say",
        lead: <IslandApp text="VS" bg="#2F80ED" />,
        trail: <IslandKeys keys={["Esc"]} />,
        body: (
          <>
            <div className="k-island__quote">In VS Code, try:</div>
            <div className="k-island__examples">
              <div>“Open the terminal and run the tests”</div>
              <div>“Find where Config is defined”</div>
              <div>“Explain this error”</div>
              <div>“Commit with message ‘fix config’”</div>
            </div>
          </>
        ),
      };
    case "plan":
      return {
        state,
        width: 490,
        label: "Plan ready",
        trail: <IslandChip>PLAN</IslandChip>,
        body: (
          <>
            <div className="k-island__quote">Clean up Downloads</div>
            <div className="k-island__plan">
              <div>
                <i>1</i>Find files older than 90 days (212 files)
              </div>
              <div>
                <i>2</i>Move installers to Downloads\Installers
              </div>
              <div>
                <i>3</i>Move the rest to Archive\2026
              </div>
              <div>
                <i>4</i>Show a summary
              </div>
            </div>
            <IslandActions
              actions={[{ label: "Approve", kind: "primary" }, { label: "Edit" }, { label: "Cancel", kind: "danger" }]}
            />
          </>
        ),
      };
    case "confirm":
      return {
        state,
        width: 490,
        label: "Needs your OK",
        lead: <IslandDot color="#FFC857" />,
        body: (
          <>
            <IslandRisk level="medium" />
            <div>
              Edit <b>src/main.rs</b> (1 line) and run <b>cargo check</b> in D:\work\kivo
            </div>
            <IslandActions
              actions={[
                { label: "Allow once", kind: "primary" },
                { label: "Allow for this project" },
                { label: "Deny", kind: "danger" },
              ]}
            />
          </>
        ),
      };
    case "high":
      return {
        state,
        width: 490,
        label: "Confirm with Windows Hello",
        lead: <IslandDot color="#FF5147" />,
        body: (
          <>
            <IslandRisk level="high" />
            <div>
              Send email to <b>maya@studio.com</b>: “Design review moved to Thursday”
            </div>
            <IslandActions
              actions={[
                { label: "Confirm", kind: "primary" },
                { label: "Cancel", kind: "danger" },
              ]}
              extraHint=". Confirm opens Windows Hello"
            />
          </>
        ),
      };
    case "draft":
      return {
        state,
        width: 500,
        label: "Prompt for Claude",
        sub: "terminal · K.I.V.O",
        lead: <IslandApp text="CC" bg="#C96442" />,
        body: (
          <>
            <div className="k-island__draft">
              Why are the runtime tests failing? Fix the cause, then re-run <b>cargo test</b>.{" "}
              <ins>Also add a test for the config loader.</ins>
            </div>
            <IslandActions
              actions={[{ label: "Send", kind: "primary" }, { label: "Edit" }, { label: "Cancel", kind: "danger" }]}
            />
            <div className="k-island__hint" style={{ marginTop: 4, paddingLeft: 25 }}>
              or “add …”, “read it back”
            </div>
          </>
        ),
      };
    case "waiting":
      return {
        state,
        width: 330,
        label: "Waiting for you",
        sub: "take your time",
        lead: <IslandDot color="#FFC857" />,
        trail: <IslandKeys keys={["Esc"]} />,
      };
    case "error":
      return {
        state,
        width: 460,
        label: "Can’t hear you",
        lead: <IslandDot color="#FF5147" />,
        body: (
          <>
            <div>USB Headset was disconnected.</div>
            <IslandActions
              actions={[{ label: "Use laptop mic", kind: "primary" }, { label: "Settings" }]}
              hint={false}
            />
          </>
        ),
      };
    case "typing":
      return {
        state,
        width: 480,
        label: "Type to KIVO",
        trail: <IslandKeys keys={["Esc"]} />,
        body: (
          <div className="k-island__input">
            <Icon name="chat" />
            <span>Summarize this page in 3 bullets</span>
            <span style={{ marginLeft: "auto", opacity: 0.5 }}>↵</span>
          </div>
        ),
      };
    case "guest":
      return { state, width: 300, label: "Guest", sub: "limited access", wave: true };
    case "paused":
      return { state, width: 220, label: "Listening paused", lead: <IslandDot color="#555" /> };
    case "media":
      return {
        state,
        width: 260,
        label: "Lo-fi Focus",
        sub: "Spotify",
        lead: <IslandApp text="S" bg="#1DB954" fg="#000" />,
        wave: true,
      };
    case "timer":
      return { state, width: 220, label: "Focus", sub: "24:12", lead: <IslandApp text="◷" bg="#FF9F0A" fg="#000" /> };
    case "download":
      return {
        state,
        width: 300,
        label: "dataset.zip",
        sub: "1.2 / 1.9 GB",
        lead: <IslandApp text="↓" bg="#3B82F6" />,
        trail: <IslandProgress value={62} />,
      };
    case "agent":
      return {
        state,
        width: 320,
        label: "Fixing tests",
        sub: "Codex · 3/5",
        lead: <IslandApp text="CX" bg="#10A37F" />,
        trail: <IslandSpin />,
      };
    case "cu":
      return {
        state,
        width: 440,
        label: "Controlling Notes",
        sub: "step 4/25 · ≈ $0.08",
        lead: <IslandApp text="▶" bg="var(--acc)" />,
        trail: (
          <>
            <button type="button" className="k-island__btn">
              Pause
            </button>
            <button type="button" className="k-island__btn k-island__btn--danger">
              Stop
            </button>
          </>
        ),
      };
    case "bypass":
      return {
        state,
        width: 420,
        label: "Bypass permissions is on",
        lead: <IslandDot color="#FF5147" />,
        trail: <IslandChip danger>58 MIN</IslandChip>,
        body: (
          <>
            <div>KIVO won’t ask before acting, including high-risk actions.</div>
            <IslandActions actions={[{ label: "Keep on" }, { label: "Turn off", kind: "primary" }]} />
          </>
        ),
      };
    case "capability-off":
      return {
        state,
        width: 420,
        label: "Screen awareness is off",
        body: (
          <>
            <div>To answer that, KIVO needs to see your screen.</div>
            <IslandActions actions={[{ label: "Turn on", kind: "primary" }, { label: "Not now" }]} />
          </>
        ),
      };
    case "in-call":
      return { state, width: 330, label: "In a call", sub: "listening quietly", lead: <IslandDot color="#555" /> };
    case "offline":
      return {
        state,
        width: 340,
        label: "Offline",
        sub: "local commands still work",
        lead: <IslandDot color="#8E8E93" />,
      };
    case "idle":
      return null;
  }
}
