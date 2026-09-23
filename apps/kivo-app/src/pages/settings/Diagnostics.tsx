/**
 * Settings → Diagnostics (UX-31): checks that everything works — microphone, wake word, speech in
 * and out, echo cancellation, brains, the browser extension and the audit log's chain. A problem
 * links to the page that fixes it.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Group, Note, Row, Spinner } from "../../components/ui";
import { Icon } from "../../icons";
import { Method } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";

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

  return (
    <>
      <div className="k-tab-actions">
        <Button icon="refresh" disabled={busy} onClick={run}>
          {t("settings.diagnostics.run")}
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
    </>
  );
}
