/**
 * Settings → Diagnostics (UX-31): checks that everything works — microphone, wake word, speech in
 * and out, echo cancellation, brains, the browser extension and the audit log's chain. A problem
 * links to the page that fixes it. "Create report" makes the diagnostics bundle (ARCH-40) and shows
 * all of it before anything is saved.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Dialog, Group, Note, Row, Spinner, useToast } from "../../components/ui";
import { Icon } from "../../icons";
import { Method } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { message } from "./useConfig";

interface Check {
  id: string;
  ok: boolean;
  title: string;
  detail: string;
  fix: string | null;
}

export function DiagnosticsTab({ onNavigate }: { onNavigate?: (page: string) => void }) {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const [checks, setChecks] = useState<Check[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<string | null>(null);
  const [making, setMaking] = useState(false);
  const toast = useToast();
  const connected = link?.status === "connected";
  const load = useCallback(
    () =>
      connected
        ? request<Check[]>(Method.diagnosticsRun)
            .then(setChecks)
            .catch(() => {})
        : Promise.resolve(),
    [connected, request],
  );
  useEffect(() => {
    void load();
  }, [load]);
  const run = () => {
    setBusy(true);
    void load().finally(() => setBusy(false));
  };
  const createReport = () => {
    setMaking(true);
    void request(Method.diagnosticsBundle)
      .then((bundle) => setReport(JSON.stringify(bundle, null, 2)))
      .catch((e: unknown) => toast(message(e)))
      .finally(() => setMaking(false));
  };
  const save = () => {
    setReport(null);
    void request<{ file: string }>(Method.diagnosticsSave)
      .then((r) => toast(t("settings.diagnostics.saved", { file: r.file })))
      .catch((e: unknown) => toast(message(e)));
  };

  return (
    <>
      <div className="k-tab-actions">
        <Button icon="refresh" disabled={busy} onClick={run}>
          {t("settings.diagnostics.run")}
        </Button>
        <Button variant="primary" disabled={!connected || making} onClick={createReport}>
          {making ? t("settings.diagnostics.making") : t("settings.diagnostics.report")}
        </Button>
      </div>
      {!checks ? (
        <Spinner label={t("settings.diagnostics.running")} />
      ) : (
        <Group>
          {checks.map((c) => (
            <Row
              key={c.id}
              lead={
                <span className={c.ok ? "k-check-ok" : "k-check-warn"} aria-hidden>
                  <Icon name={c.ok ? "check" : "warning"} />
                </span>
              }
              title={c.title}
              subtitle={c.detail}
              end={
                !c.ok && c.fix ? (
                  <Button size="sm" onClick={() => onNavigate?.(c.fix ?? "home")}>
                    {t("settings.diagnostics.fix")}
                  </Button>
                ) : (
                  <span className="k-visually-hidden">
                    {c.ok ? t("settings.diagnostics.ok") : t("settings.diagnostics.problem")}
                  </span>
                )
              }
            />
          ))}
        </Group>
      )}
      <Note>{t("settings.diagnostics.note")}</Note>
      <Dialog
        open={report !== null}
        onOpenChange={(o) => !o && setReport(null)}
        title={t("settings.diagnostics.reportTitle")}
        description={t("settings.diagnostics.reportDetail")}
        footer={
          <>
            <Button variant="plain" onClick={() => setReport(null)}>
              {t("ui.cancel")}
            </Button>
            <Button variant="primary" icon="download" onClick={save}>
              {t("settings.diagnostics.save")}
            </Button>
          </>
        }
      >
        <pre className="k-report" aria-label={t("settings.diagnostics.reportTitle")}>
          {report}
        </pre>
      </Dialog>
    </>
  );
}
