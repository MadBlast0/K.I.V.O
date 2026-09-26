/**
 * What happens when you talk to KIVO, as one small moving picture (UX §4, owner 2026-09-26): a
 * face speaks, its words travel to the brain, the brain's answer travels to the speaker, which
 * says it. The stage the page sets up is lit, the others rest, and a line under it says what
 * that part does. CSS only (transform and opacity); reduced motion shows it still.
 */
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../../icons";
import { cn } from "../../lib/cn";

export type FlowStage = "listen" | "brain" | "speak";

export function VoiceFlow({ at }: { at: FlowStage }) {
  const { t } = useTranslation();
  const node = (stage: FlowStage, art: ReactNode) => (
    <div className={cn("k-flow__node", stage === at && "is-on")} data-stage={stage}>
      <div className="k-flow__art">{art}</div>
      <span className="k-flow__label">{t(`onboarding.flow.${stage}`)}</span>
    </div>
  );
  return (
    <figure className="k-flow" aria-label={t("onboarding.flow.label")}>
      <div className="k-flow__row" aria-hidden>
        {node(
          "listen",
          <>
            <svg className="k-flow__face" viewBox="0 0 48 48">
              <circle cx="24" cy="24" r="17" />
              <circle className="k-flow__eye" cx="18" cy="21" r="1.8" />
              <circle className="k-flow__eye" cx="30" cy="21" r="1.8" />
              <ellipse className="k-flow__mouth" cx="24" cy="31" rx="4.5" ry="2.6" />
            </svg>
            <span className="k-flow__waves k-flow__waves--out">
              <i />
              <i />
            </span>
          </>,
        )}
        <div className="k-flow__link">
          <span className="k-flow__line" />
          <span className="k-flow__bubble k-flow__bubble--ask">{t("onboarding.flow.ask")}</span>
        </div>
        {node(
          "brain",
          <span className="k-flow__brain">
            <Icon name="brain" />
          </span>,
        )}
        <div className="k-flow__link">
          <span className="k-flow__line" />
          <span className="k-flow__bubble k-flow__bubble--answer">{t("onboarding.flow.answer")}</span>
        </div>
        {node(
          "speak",
          <>
            <span className="k-flow__speaker">
              <Icon name="volume" />
            </span>
            <span className="k-flow__waves k-flow__waves--speak">
              <i />
              <i />
            </span>
          </>,
        )}
      </div>
      <figcaption className="k-flow__caption">{t(`onboarding.flow.explain.${at}`)}</figcaption>
    </figure>
  );
}
