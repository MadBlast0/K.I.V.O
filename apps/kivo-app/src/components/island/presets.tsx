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
import i18n from "../../i18n";
import { withNodes } from "../../i18n/nodes";

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

export const ISLAND_STATES: ReadonlyArray<IslandState> = [
  "listening",
  "followup",
  "thinking",
  "acting",
  "speaking",
  "undo",
  "help",
  "plan",
  "confirm",
  "high",
  "draft",
  "waiting",
  "error",
  "typing",
  "guest",
  "paused",
];

/** Short notices the Island shows on its own (mockup → System surfaces → Island notices). */
export const ISLAND_NOTICES: ReadonlyArray<IslandState> = ["bypass", "capability-off", "in-call", "offline"];

export const ISLAND_LIVE: ReadonlyArray<IslandState> = ["media", "timer", "download", "agent", "cu", "idle"];

/** The gallery chip for a state, translated. */
export function islandStateLabel(state: IslandState): string {
  return i18n.t(`demo.chip.${state}`);
}

export function islandPreset(
  state: IslandState,
  opts: { partial?: string; mode?: "ask" | "edits" | "plan" | "auto" | "bypass" } = {},
): IslandModel | null {
  const t = i18n.t.bind(i18n);
  const REQ = t("demo.request");
  const ANS = t("demo.answer");
  const mode = opts.mode ?? "auto";
  const modeChip =
    mode === "bypass" ? (
      <IslandChip danger>{t("demo.modeChip.bypass")}</IslandChip>
    ) : mode === "auto" ? null : (
      <IslandChip>{t(`demo.modeChip.${mode}`)}</IslandChip>
    );
  const p = opts.partial ?? "";

  switch (state) {
    case "listening":
      return {
        state,
        width: p ? 470 : 236,
        label: t("demo.listening"),
        wave: true,
        body: p ? <div>{p}</div> : undefined,
      };
    case "followup":
      return { state, width: 300, label: t("demo.listening"), sub: t("demo.forAFollowUp"), trail: <IslandRing /> };
    case "thinking":
      return { state, width: 196, label: t("demo.thinking"), trail: <IslandSpin /> };
    case "acting":
      return {
        state,
        width: 480,
        label: t("demo.working"),
        sub: t("demo.n2Steps"),
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
                {withNodes(t, "demo.stepSpotify", { app: <b>{t("demo.spotify")}</b> })}
              </div>
              <div>
                <IslandSpin />
                {withNodes(t, "demo.stepReminder", { app: <b>{t("demo.reminders")}</b> })}
              </div>
            </div>
          </>
        ),
      };
    case "speaking":
      return {
        state,
        width: 480,
        label: t("demo.kivo"),
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
        label: t("demo.moved3FilesToArchive"),
        lead: <IslandApp text="▣" bg="#3B82F6" />,
        trail: (
          <>
            <button type="button" className="k-island__btn k-island__btn--primary">
              {t("demo.undo")}
            </button>
            <IslandRing />
          </>
        ),
      };
    case "help":
      return {
        state,
        width: 480,
        label: t("demo.whatYouCanSay"),
        lead: <IslandApp text="VS" bg="#2F80ED" />,
        trail: <IslandKeys keys={["Esc"]} />,
        body: (
          <>
            <div className="k-island__quote">{t("demo.inVsCode")}</div>
            <div className="k-island__examples">
              <div>{t("demo.openTheTerminalAndRun")}</div>
              <div>{t("demo.findWhereConfigIsDefined")}</div>
              <div>{t("demo.explainThisError")}</div>
              <div>{t("demo.commitWithMessageFixConfig")}</div>
            </div>
          </>
        ),
      };
    case "plan":
      return {
        state,
        width: 490,
        label: t("demo.planReady"),
        trail: <IslandChip>{t("demo.modeChip.plan")}</IslandChip>,
        body: (
          <>
            <div className="k-island__quote">{t("demo.cleanUpDownloads")}</div>
            <div className="k-island__plan">
              <div>
                <i>1</i>
                {t("demo.findOldFiles")}
              </div>
              <div>
                <i>2</i>
                {t("demo.moveInstallersToDownloadsInstallers")}
              </div>
              <div>
                <i>3</i>
                {t("demo.moveTheRestToArchive")}
              </div>
              <div>
                <i>4</i>
                {t("demo.showASummary")}
              </div>
            </div>
            <IslandActions
              actions={[
                { label: t("demo.approve"), kind: "primary" },
                { label: t("demo.edit") },
                { label: t("demo.cancel"), kind: "danger" },
              ]}
            />
          </>
        ),
      };
    case "confirm":
      return {
        state,
        width: 490,
        label: t("demo.needsYourOk"),
        lead: <IslandDot color="#FFC857" />,
        body: (
          <>
            <IslandRisk level="medium" />
            <div>{withNodes(t, "demo.confirmEdit", { file: <b>src/main.rs</b>, command: <b>cargo check</b> })}</div>
            <IslandActions
              actions={[
                { label: t("demo.allowOnce"), kind: "primary" },
                { label: t("demo.allowForThisProject") },
                { label: t("demo.deny"), kind: "danger" },
              ]}
            />
          </>
        ),
      };
    case "high":
      return {
        state,
        width: 490,
        label: t("demo.confirmWithWindowsHello"),
        lead: <IslandDot color="#FF5147" />,
        body: (
          <>
            <IslandRisk level="high" />
            <div>{withNodes(t, "demo.confirmEmail", { to: <b>maya@studio.com</b> })}</div>
            <IslandActions
              actions={[
                { label: t("demo.confirm"), kind: "primary" },
                { label: t("demo.cancel"), kind: "danger" },
              ]}
              extraHint={t("demo.confirmOpensHello")}
            />
          </>
        ),
      };
    case "draft":
      return {
        state,
        width: 500,
        label: t("demo.promptForClaude"),
        sub: t("demo.terminalKIVO"),
        lead: <IslandApp text="CC" bg="#C96442" />,
        body: (
          <>
            <div className="k-island__draft">
              {withNodes(t, "demo.draft", { command: <b>cargo test</b> })} <ins>{t("demo.alsoAddATestFor")}</ins>
            </div>
            <IslandActions
              actions={[
                { label: t("demo.send"), kind: "primary" },
                { label: t("demo.edit") },
                { label: t("demo.cancel"), kind: "danger" },
              ]}
            />
            <div className="k-island__hint" style={{ marginTop: 4, paddingInlineStart: 25 }}>
              {t("demo.orAddReadItBack")}
            </div>
          </>
        ),
      };
    case "waiting":
      return {
        state,
        width: 330,
        label: t("demo.waitingForYou"),
        sub: t("demo.takeYourTime"),
        lead: <IslandDot color="#FFC857" />,
        trail: <IslandKeys keys={["Esc"]} />,
      };
    case "error":
      return {
        state,
        width: 460,
        label: t("demo.canTHearYou"),
        lead: <IslandDot color="#FF5147" />,
        body: (
          <>
            <div>{t("demo.usbHeadsetWasDisconnected")}</div>
            <IslandActions
              actions={[{ label: t("demo.useLaptopMic"), kind: "primary" }, { label: t("demo.settings") }]}
              hint={false}
            />
          </>
        ),
      };
    case "typing":
      return {
        state,
        width: 480,
        label: t("demo.typeToKivo"),
        trail: <IslandKeys keys={["Esc"]} />,
        body: (
          <div className="k-island__input">
            <Icon name="chat" />
            <span>{t("demo.summarizeThisPageIn3")}</span>
            <span style={{ marginInlineStart: "auto", opacity: 0.5 }}>↵</span>
          </div>
        ),
      };
    case "guest":
      return { state, width: 300, label: t("demo.guest"), sub: t("demo.limitedAccess"), wave: true };
    case "paused":
      return { state, width: 220, label: t("demo.listeningPaused"), lead: <IslandDot color="#555" /> };
    case "media":
      return {
        state,
        width: 260,
        label: t("demo.loFiFocus"),
        sub: t("demo.spotify"),
        lead: <IslandApp text="S" bg="#1DB954" fg="#000" />,
        wave: true,
      };
    case "timer":
      return {
        state,
        width: 220,
        label: t("demo.focus"),
        sub: "24:12",
        lead: <IslandApp text="◷" bg="#FF9F0A" fg="#000" />,
      };
    case "download":
      return {
        state,
        width: 300,
        label: t("demo.datasetZip"),
        sub: t("demo.downloadSize", { done: 1.2, total: 1.9 }),
        lead: <IslandApp text="↓" bg="#3B82F6" />,
        trail: <IslandProgress value={62} />,
      };
    case "agent":
      return {
        state,
        width: 320,
        label: t("demo.fixingTests"),
        sub: t("demo.codex35"),
        lead: <IslandApp text="CX" bg="#10A37F" />,
        trail: <IslandSpin />,
      };
    case "cu":
      return {
        state,
        width: 440,
        label: t("demo.controllingNotes"),
        sub: t("demo.step425008"),
        lead: <IslandApp text="▶" bg="var(--acc)" />,
        trail: (
          <>
            <button type="button" className="k-island__btn">
              {t("demo.pause")}
            </button>
            <button type="button" className="k-island__btn k-island__btn--danger">
              {t("demo.stop")}
            </button>
          </>
        ),
      };
    case "bypass":
      return {
        state,
        width: 420,
        label: t("demo.bypassPermissionsIsOn"),
        lead: <IslandDot color="#FF5147" />,
        trail: <IslandChip danger>{t("demo.n58Min")}</IslandChip>,
        body: (
          <>
            <div>{t("demo.kivoWonTAskBefore")}</div>
            <IslandActions actions={[{ label: t("demo.keepOn") }, { label: t("demo.turnOff"), kind: "primary" }]} />
          </>
        ),
      };
    case "capability-off":
      return {
        state,
        width: 420,
        label: t("demo.screenAwarenessIsOff"),
        body: (
          <>
            <div>{t("demo.toAnswerThatKivoNeeds")}</div>
            <IslandActions actions={[{ label: t("demo.turnOn"), kind: "primary" }, { label: t("demo.notNow") }]} />
          </>
        ),
      };
    case "in-call":
      return {
        state,
        width: 330,
        label: t("demo.inACall"),
        sub: t("demo.listeningQuietly"),
        lead: <IslandDot color="#555" />,
      };
    case "offline":
      return {
        state,
        width: 340,
        label: t("demo.offline"),
        sub: t("demo.localCommandsStillWork"),
        lead: <IslandDot color="#8E8E93" />,
      };
    case "idle":
      return null;
  }
}
