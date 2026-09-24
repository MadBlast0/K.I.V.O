/**
 * Installing a CLI agent or a tool KIVO uses (DISC-07, DIST-14): first what will be installed,
 * why, and the exact commands; nothing runs until Install is pressed. Then each step's progress
 * with its output (folded away until wanted), the version once it answers, and for an agent its
 * own sign-in and a test. A step KIVO can't do (a vendor download) says so and links there.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Method, type InstallPlan, type InstallStepView, type InstallView } from "../../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../../ipc/runtime";
import { Button, Dialog, Note, Spinner, Tag, useToast } from "../ui";

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

const TONE: Record<string, "success" | "danger" | "warning" | undefined> = {
  done: "success",
  failed: "danger",
  needsYou: "warning",
};

function StepRow({ step, onVendor }: { step: InstallStepView; onVendor: (url: string) => void }) {
  const { t } = useTranslation();
  const url = step.why.match(/https:\/\/\S+/)?.[0];
  return (
    <li className="k-install__step" data-status={step.status}>
      <div className="k-install__head">
        {step.status === "running" ? (
          <Spinner label={t("install.running")} />
        ) : (
          <Tag tone={TONE[step.status]}>{t(`install.status.${step.status}`)}</Tag>
        )}
        <b>{step.title}</b>
      </div>
      <p className="k-meta">{url ? step.why.replace(url, "").trim() : step.why}</p>
      {step.command ? (
        <code className="k-install__command">{step.command}</code>
      ) : (
        url && (
          <Button size="sm" icon="external" onClick={() => onVendor(url)}>
            {t("install.openWebsite")}
          </Button>
        )
      )}
      {step.output && (
        <details className="k-install__output">
          <summary>{t("install.output")}</summary>
          <pre>{step.output}</pre>
        </details>
      )}
    </li>
  );
}

export function InstallSheet({
  id,
  agent,
  onClose,
  onDone,
}: {
  id: string;
  /** A CLI agent: sign-in and a test follow the install. */
  agent: boolean;
  onClose: () => void;
  onDone: () => void;
}) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [plan, setPlan] = useState<InstallPlan | null>(null);
  const [run, setRun] = useState<InstallView | null>(null);
  // The parent's callbacks change every render; the effects below only follow `id`.
  const callbacks = useRef({ onClose, onDone, toast });
  useEffect(() => {
    callbacks.current = { onClose, onDone, toast };
  });

  useEffect(() => {
    void request<InstallPlan>(Method.installsPlan, { id })
      .then(setPlan)
      .catch((e: unknown) => {
        callbacks.current.toast(message(e));
        callbacks.current.onClose();
      });
  }, [id, request]);
  const refresh = useCallback(() => {
    void request<InstallView | null>(Method.installsStatus, { id })
      .then((v) => {
        setRun(v);
        if (v?.status === "done") callbacks.current.onDone();
      })
      .catch(() => {});
  }, [id, request]);
  useRuntimeEvents((event) => {
    if (event.group === "system" && event.event.type === "discoveryChanged" && event.event.section === "installs") {
      refresh();
    }
  });

  const install = () => {
    if (!plan) return;
    void request(Method.installsStart, { id, commands: plan.steps.map((s) => s.command).filter(Boolean) })
      .then(refresh)
      .catch((e: unknown) => toast(message(e)));
  };
  const vendor = (url: string) => void request(Method.systemOpenUrl, { url }).catch(() => {});
  const steps = run?.steps ?? plan?.steps ?? [];
  const status = run?.status;
  const name = plan?.name ?? id;

  const footer = !plan ? null : plan.installed || status === "done" ? (
    <>
      {agent && (
        <>
          <Button onClick={() => void request(Method.brainsSignIn, { id }).catch((e: unknown) => toast(message(e)))}>
            {t("install.signIn")}
          </Button>
          <Button
            onClick={() =>
              void request(Method.brainsCheck, { id })
                .then(() => toast(t("install.tested", { name })))
                .catch((e: unknown) => toast(message(e)))
            }
          >
            {t("install.test")}
          </Button>
        </>
      )}
      <Button variant="primary" onClick={onClose}>
        {t("install.close")}
      </Button>
    </>
  ) : status === "running" ? (
    <Button onClick={() => void request(Method.installsCancel, { id })}>{t("ui.cancel")}</Button>
  ) : (
    <>
      <Button variant="plain" onClick={onClose}>
        {t("ui.cancel")}
      </Button>
      <Button variant="primary" icon="download" onClick={install}>
        {status === "failed" ? t("install.retry") : t("install.install")}
      </Button>
    </>
  );

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title={t("install.title", { name })}
      description={
        plan?.installed || status === "done"
          ? t("install.installed", { name, version: run?.version ?? plan?.version ?? "" })
          : t("install.explain")
      }
      footer={footer}
    >
      {!plan ? (
        <Spinner label={t("install.checking")} />
      ) : (
        <>
          {steps.length > 0 && (
            <ol className="k-install" aria-label={t("install.steps")}>
              {steps.map((s) => (
                <StepRow key={s.id} step={s} onVendor={vendor} />
              ))}
            </ol>
          )}
          {run?.error && <Note>{run.error}</Note>}
          {!run && !plan.installed && <Note>{t("install.consent")}</Note>}
        </>
      )}
    </Dialog>
  );
}
