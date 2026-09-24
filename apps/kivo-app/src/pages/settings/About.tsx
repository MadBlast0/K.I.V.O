/**
 * Settings → About (UX-31, DIST-13): the version, and every library's and model's licence and
 * attribution. Libraries come from `generated/licenses.json` (`pnpm licenses:gen`: the crates the
 * three programs are built from, and the app's npm packages); models from the model manifest.
 * Updates: the version, Check now / Install now, the channel and when to install (DIST-07/08).
 */
import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { UpdatesSection } from "../../components/updates/Updates";
import { Button, Dialog, Group, Mark, Meta, PageTabs, Row, SearchField, Section, useToast } from "../../components/ui";
import licenses from "../../generated/licenses.json";
import { Method, type ModelItem } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { message } from "./useConfig";

const REPO = "https://github.com/MadBlast0/K.I.V.O";

interface Library {
  name: string;
  version: string;
  license: string;
}

function Licenses({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const [tab, setTab] = useState<"libraries" | "models">("libraries");
  const [query, setQuery] = useState("");
  const [models, setModels] = useState<ModelItem[]>([]);
  useEffect(() => {
    if (!open || link?.status !== "connected") return;
    void request<ModelItem[]>(Method.modelsList)
      .then(setModels)
      .catch(() => {});
  }, [open, link?.status, request]);
  const all: Library[] = useMemo(() => [...licenses.rust, ...licenses.npm], []);
  const q = query.trim().toLowerCase();
  const shown = all.filter((l) => !q || `${l.name} ${l.license}`.toLowerCase().includes(q));
  const counts = useMemo(() => {
    const byLicense = new Map<string, number>();
    for (const l of all) byLicense.set(l.license, (byLicense.get(l.license) ?? 0) + 1);
    return [...byLicense.entries()].toSorted((a, b) => b[1] - a[1]);
  }, [all]);

  return (
    <Dialog
      open={open}
      onOpenChange={(o) => !o && onClose()}
      title={t("settings.about.licenses")}
      description={t("settings.about.licensesDetail", { count: all.length })}
    >
      <PageTabs<"libraries" | "models">
        label={t("settings.about.licenses")}
        value={tab}
        onChange={setTab}
        tabs={[
          {
            value: "libraries",
            label: t("settings.about.libraries"),
            content: (
              <>
                <p className="k-meta">{counts.map(([l, n]) => `${l} (${n})`).join(" · ")}</p>
                <Button
                  size="sm"
                  onClick={() => void request(Method.aboutNotices).catch((e: unknown) => toast(message(e)))}
                >
                  {t("settings.about.fullTexts")}
                </Button>
                <SearchField
                  aria-label={t("settings.about.search")}
                  placeholder={t("settings.about.search")}
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                />
                <div className="k-license-list">
                  {shown.map((l) => (
                    <div key={`${l.name}@${l.version}`} className="k-license">
                      <span>
                        {l.name} <Meta>{l.version}</Meta>
                      </span>
                      <Meta>{l.license}</Meta>
                    </div>
                  ))}
                </div>
              </>
            ),
          },
          {
            value: "models",
            label: t("settings.about.models"),
            content: (
              <Group>
                {models.length === 0 ? (
                  <Row title={t("settings.about.noModels")} />
                ) : (
                  models.map((m) => (
                    <Row
                      key={m.id}
                      title={m.name}
                      subtitle={[m.attribution, m.source].filter(Boolean).join(" · ")}
                      end={<Meta>{m.license}</Meta>}
                    />
                  ))
                )}
              </Group>
            ),
          },
        ]}
      />
    </Dialog>
  );
}

export function AboutTab() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const [licensesOpen, setLicensesOpen] = useState(false);
  const open = (url: string) => void request(Method.systemOpenUrl, { url }).catch((e: unknown) => toast(message(e)));
  const version = link?.runtimeVersion ?? "—";
  return (
    <>
      <div className="k-about">
        <Mark size={72} />
        <h2>KIVO</h2>
        <p>
          {t("settings.about.tagline")}
          <br />
          {t("settings.about.version", { version })}
        </p>
      </div>
      <div className="k-narrow">
        <UpdatesSection />
        <Section title={t("settings.about.more")} />
        <Group>
          <Row
            icon="file"
            title={t("settings.about.licenses")}
            subtitle={t("settings.about.licensesHint")}
            onClick={() => setLicensesOpen(true)}
          />
          <Row
            icon="permissions"
            title={t("settings.about.security")}
            onClick={() => open(`${REPO}/blob/main/SECURITY.md`)}
          />
          <Row
            icon="code"
            title={t("settings.about.source")}
            subtitle="github.com/MadBlast0/K.I.V.O"
            onClick={() => open(REPO)}
          />
        </Group>
      </div>
      <Licenses open={licensesOpen} onClose={() => setLicensesOpen(false)} />
    </>
  );
}
