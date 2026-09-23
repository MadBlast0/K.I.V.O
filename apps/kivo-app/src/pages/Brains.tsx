/**
 * Brains (UX-22, BRAINS §4–5, DISCOVERY §1–3): the brains KIVO can think with. No-key sign-in comes
 * first (CLI agents with their own login, "Connect with OpenRouter", local servers found on this
 * PC); API keys are the advanced option, write-only and tested by the runtime (BRAIN-17/18). Free
 * options are labelled (CONV-08). The Context tab shows what a conversation starts with (CONV-30).
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import {
  Alert,
  Button,
  Dialog,
  Group,
  IconButton,
  Monogram,
  NewDot,
  Note,
  PageTabs,
  Pill,
  RadioGroup,
  Radio,
  Row,
  Section,
  Select,
  Sheet,
  Spinner,
  Switch,
  Tag,
  TextArea,
  TextField,
  useToast,
} from "../components/ui";
import {
  brainColor,
  healthTone,
  monogram,
  type BrainView,
  type BrainsList,
  type Catalog,
  type ContextPreview,
  type DiscoverySection,
  type Health,
  type Profile,
} from "../ipc/brains";
import { Method } from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";

type Tab = "brains" | "context";

export function Brains() {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("brains");
  return (
    <>
      <PageHeader title={t("nav.brains")} subtitle={t("brains.subtitle")} />
      <PageTabs<Tab>
        label={t("nav.brains")}
        value={tab}
        onChange={setTab}
        tabs={[
          { value: "brains", label: t("brains.tab"), content: <BrainsTab /> },
          { value: "context", label: t("context.tab"), content: <ContextTab /> },
        ]}
      />
    </>
  );
}

function useFail() {
  const toast = useToast();
  return useCallback((e: unknown) => toast(e instanceof Error ? e.message : String(e)), [toast]);
}

/** "Checked 2 min ago" from milliseconds since the epoch. */
function useAgo() {
  const { t, i18n } = useTranslation();
  return useCallback(
    (ms: number | null) => {
      if (ms === null) return t("brains.neverChecked");
      const minutes = Math.max(0, Math.round((Date.now() - ms) / 60_000));
      const rtf = new Intl.RelativeTimeFormat(i18n.language, { numeric: "auto" });
      return t("brains.checked", {
        when: minutes < 60 ? rtf.format(-minutes, "minute") : rtf.format(-Math.round(minutes / 60), "hour"),
      });
    },
    [i18n.language, t],
  );
}

export function healthLabel(t: (k: string, o?: Record<string, unknown>) => string, h: Health): string {
  return h.state === "unreachable"
    ? t("brains.health.unreachable", { reason: h.reason })
    : t(`brains.health.${h.state}`);
}

function BrainsTab() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const fail = useFail();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [list, setList] = useState<BrainsList | null>(null);
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [cli, setCli] = useState<DiscoverySection | null>(null);
  const [local, setLocal] = useState<DiscoverySection | null>(null);
  const [refreshing, setRefreshing] = useState<string | null>(null);
  const [open, setOpen] = useState<BrainView | null>(null);
  const [adding, setAdding] = useState(false);
  const [profile, setProfile] = useState<Profile | null>(null);
  const ago = useAgo();

  const load = useCallback(() => {
    if (!connected) return;
    void request<BrainsList>(Method.brainsList)
      .then(setList)
      .catch(() => {});
    void request<DiscoverySection>(Method.brainsDiscovery, { section: "cli" })
      .then(setCli)
      .catch(() => {});
    void request<DiscoverySection>(Method.brainsDiscovery, { section: "local" })
      .then(setLocal)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  useEffect(() => {
    if (!connected) return;
    void request<Catalog>(Method.brainsCatalog)
      .then(setCatalog)
      .catch(() => {});
  }, [connected, request]);
  // Health and discovery are pushed as they change (DISCOVERY §3); health is re-checked every
  // five minutes while this page is visible (DISC-14).
  useRuntimeEvents((event) => {
    if (event.group === "provider") load();
  });
  useEffect(() => {
    if (!connected) return;
    const id = window.setInterval(() => {
      if (document.visibilityState === "visible") load();
    }, 300_000);
    return () => window.clearInterval(id);
  }, [connected, load]);
  // The user has seen what was found: the New labels go (DISC-12).
  useEffect(() => {
    if (!connected) return;
    const timer = window.setTimeout(() => {
      void request(Method.brainsViewed, { section: "cli" }).catch(() => {});
      void request(Method.brainsViewed, { section: "local" }).catch(() => {});
    }, 3_000);
    return () => window.clearTimeout(timer);
  }, [connected, request]);

  const refresh = (section: "cli" | "local") => {
    setRefreshing(section);
    request<DiscoverySection>(Method.brainsRefresh, { section })
      .then((s) => (section === "cli" ? setCli(s) : setLocal(s)))
      .catch(fail)
      .finally(() => setRefreshing(null));
  };
  const connect = (id: string, extra: Record<string, unknown> = {}) =>
    request<{ connected: BrainView[] }>(Method.brainsConnect, { id, ...extra })
      .then(() => {
        load();
        toast(t("brains.added"));
      })
      .catch(fail);
  const signIn = (id: string) =>
    request<{ started?: boolean }>(Method.brainsSignIn, { id })
      .then((r) => {
        toast(r.started ? t("brains.signInStarted") : t("brains.signedIn"));
        load();
      })
      .catch(fail);

  const isConnected = (id: string) => list?.connected.some((c) => c.id === id) ?? false;
  const entry = (id: string) => catalog?.brains.find((b) => b.id === id);
  const foundCli = cli?.items.filter((i) => !isConnected(i.id)) ?? [];
  const foundLocal = local?.items.filter((i) => !isConnected(i.id)) ?? [];
  const missing =
    catalog?.cli.filter((tool) => !cli?.items.some((i) => i.id === tool.id) && !isConnected(tool.id)) ?? [];

  if (!connected) return <Note>{t("brains.notConnected")}</Note>;
  if (!list) return <Spinner label={t("brains.loading")} />;

  return (
    <div className="k-brains">
      <div className="k-brains__actions">
        <Button variant="primary" icon="add" onClick={() => setAdding(true)}>
          {t("brains.add")}
        </Button>
      </div>
      {!list.cloudOn && <Alert kind="info" title={t("brains.cloudOff")} />}

      <Section title={t("brains.connected")} />
      {list.connected.length === 0 ? (
        <Note>{t("brains.noneYet")}</Note>
      ) : (
        <Group>
          {list.connected.map((b) => (
            <Row
              key={b.id}
              lead={<Monogram text={monogram(b.name)} color={brainColor(b.id)} />}
              title={b.name}
              subtitle={
                b.privacy === "local"
                  ? t("brains.onThisPc", { count: b.models.length })
                  : b.kind === "cli"
                    ? t("brains.viaAcp")
                    : b.models.length > 0
                      ? t("brains.models", { count: b.models.length })
                      : b.hasKey
                        ? t("brains.ownKey")
                        : t("brains.noKey")
              }
              end={
                <>
                  {(b.free || b.privacy === "local") && <Pill tone="success">{t("brains.free")}</Pill>}
                  <Tag tone={healthTone(b.health)}>{healthLabel(t, b.health)}</Tag>
                </>
              }
              onClick={() => setOpen(b)}
            />
          ))}
        </Group>
      )}

      <Section
        title={t("brains.foundAgents")}
        aside={
          <span className="k-brains__refresh">
            <span className="k-meta">{ago(cli?.checkedAt ?? null)}</span>
            <Button
              size="sm"
              variant="plain"
              icon="refresh"
              disabled={refreshing !== null}
              onClick={() => refresh("cli")}
            >
              {refreshing === "cli" ? t("brains.refreshing") : t("brains.refresh")}
            </Button>
          </span>
        }
      />
      {!list.cliAgentsOn && foundCli.length > 0 && <Note>{t("brains.cliAgentsOff")}</Note>}
      {foundCli.length === 0 ? (
        <Note>{t("brains.noAgentsFound")}</Note>
      ) : (
        <Group>
          {foundCli.map((i) => {
            const name = i.data.name ?? i.id;
            const status = i.data.needsAdapter
              ? t("brains.needsAdapter")
              : i.data.signedIn === false
                ? t("brains.needsSignIn")
                : i.data.signedIn
                  ? t("brains.signedInFound")
                  : t("brains.installed");
            return (
              <Row
                key={i.id}
                lead={<Monogram text={monogram(name)} color={brainColor(i.id)} />}
                title={
                  <>
                    {name}
                    {i.new && <NewDot />}
                  </>
                }
                subtitle={[status, i.data.version, i.data.free].filter(Boolean).join(" · ")}
                end={
                  <>
                    {i.data.free && <Pill tone="success">{t("brains.free")}</Pill>}
                    {i.data.needsAdapter ? (
                      <Button
                        size="sm"
                        onClick={() => void navigator.clipboard?.writeText(i.data.adapterInstall ?? "")}
                      >
                        {t("brains.copyInstall")}
                      </Button>
                    ) : i.data.signedIn === false ? (
                      <Button size="sm" onClick={() => void signIn(i.id)}>
                        {t("brains.signIn")}
                      </Button>
                    ) : (
                      <Button size="sm" variant="primary" onClick={() => void connect(i.id)}>
                        {t("brains.use")}
                      </Button>
                    )}
                  </>
                }
              />
            );
          })}
        </Group>
      )}
      {foundCli.some((i) => i.data.needsAdapter) && <Note>{t("brains.adapterNote")}</Note>}

      <Section
        title={t("brains.foundLocal")}
        aside={
          <span className="k-brains__refresh">
            <span className="k-meta">{ago(local?.checkedAt ?? null)}</span>
            <Button
              size="sm"
              variant="plain"
              icon="refresh"
              disabled={refreshing !== null}
              onClick={() => refresh("local")}
            >
              {refreshing === "local" ? t("brains.refreshing") : t("brains.refresh")}
            </Button>
          </span>
        }
      />
      {foundLocal.length === 0 ? (
        <Note>{t("brains.noLocalFound")}</Note>
      ) : (
        <Group>
          {foundLocal.map((i) => (
            <Row
              key={i.id}
              lead={<Monogram text={monogram(i.data.name ?? i.id)} color={brainColor(i.id)} />}
              title={
                <>
                  {i.data.name ?? i.id}
                  {i.new && <NewDot />}
                </>
              }
              subtitle={t("brains.runningHere", { count: i.data.models?.length ?? 0 })}
              end={
                <>
                  <Pill tone="success">{t("brains.free")}</Pill>
                  <Button size="sm" variant="primary" onClick={() => void connect(i.id, { baseUrl: i.data.url ?? "" })}>
                    {t("brains.use")}
                  </Button>
                </>
              }
            />
          ))}
        </Group>
      )}

      {missing.length > 0 && (
        <>
          <Section title={t("brains.notInstalled")} />
          <Group>
            {missing.map((tool) => {
              const e = entry(tool.id);
              return (
                <Row
                  key={tool.id}
                  lead={<Monogram text={monogram(e?.name ?? tool.id)} color={brainColor(tool.id)} />}
                  title={e?.name ?? tool.id}
                  subtitle={<code>{tool.install}</code>}
                  end={
                    <>
                      {e?.free && <Pill tone="success">{t("brains.free")}</Pill>}
                      <Button size="sm" onClick={() => void navigator.clipboard?.writeText(tool.install)}>
                        {t("brains.copyInstall")}
                      </Button>
                    </>
                  }
                />
              );
            })}
          </Group>
        </>
      )}

      <Section title={t("brains.profiles")} />
      <Group>
        <RadioGroup
          value={list.defaultProfile}
          onChange={(v) => void request(Method.brainsSetDefault, { profile: v }).then(load).catch(fail)}
          label={t("brains.defaultProfile")}
        >
          {list.profiles.map((p) => (
            <Row
              key={p.id}
              lead={<Radio value={p.id} label={p.name} />}
              title={p.name}
              subtitle={profileLine(t, p, list.connected)}
              end={
                <Button size="sm" variant="plain" onClick={() => setProfile(p)}>
                  {t("brains.edit")}
                </Button>
              }
            />
          ))}
        </RadioGroup>
      </Group>
      <Note>{t("brains.freeNote")}</Note>
      <Note>{t("brains.routingNote")}</Note>

      <Section title={t("brains.agentFolder")} />
      <Group>
        <Row
          icon="folder"
          title={t("brains.agentFolderTitle")}
          subtitle={list.workspace}
          end={
            <WorkspaceEdit
              value={list.workspace}
              onSave={(folder) => request(Method.brainsSetWorkspace, { folder }).then(load)}
            />
          }
        />
      </Group>

      {open && (
        <BrainSheet
          brain={list.connected.find((b) => b.id === open.id) ?? open}
          apiKey={entry(open.id)?.signIn === "apiKey" || !entry(open.id)}
          onClose={() => setOpen(null)}
          onChanged={load}
        />
      )}
      {adding && catalog && <AddBrain catalog={catalog} onClose={() => setAdding(false)} onAdded={load} />}
      {profile && (
        <ProfileSheet profile={profile} brains={list.connected} onClose={() => setProfile(null)} onSaved={load} />
      )}
    </div>
  );
}

function profileLine(t: (k: string, o?: Record<string, unknown>) => string, p: Profile, brains: BrainView[]) {
  const name = (id: string) => brains.find((b) => b.id === id)?.name ?? id;
  const primary = p.primary
    ? `${name(p.primary.provider)}${p.primary.model ? ` · ${p.primary.model}` : ""}`
    : t("brains.auto");
  const privacy = p.privacy === "strictPrivate" ? t("brains.staysLocal") : "";
  return [primary, privacy].filter(Boolean).join(" · ");
}

function WorkspaceEdit({ value, onSave }: { value: string; onSave: (folder: string) => Promise<unknown> }) {
  const { t } = useTranslation();
  const fail = useFail();
  const [open, setOpen] = useState(false);
  const [folder, setFolder] = useState(value);
  return (
    <Dialog
      open={open}
      onOpenChange={setOpen}
      trigger={<Button size="sm">{t("brains.change")}</Button>}
      title={t("brains.agentFolderTitle")}
      description={t("brains.agentFolderHint")}
      footer={
        <Button
          variant="primary"
          onClick={() =>
            void onSave(folder)
              .then(() => setOpen(false))
              .catch(fail)
          }
        >
          {t("brains.save")}
        </Button>
      }
    >
      <TextField value={folder} onChange={(e) => setFolder(e.target.value)} aria-label={t("brains.agentFolderTitle")} />
    </Dialog>
  );
}

function BrainSheet({
  brain,
  apiKey,
  onClose,
  onChanged,
}: {
  brain: BrainView;
  apiKey: boolean;
  onClose: () => void;
  onChanged: () => void;
}) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const fail = useFail();
  const toast = useToast();
  const [testing, setTesting] = useState(false);
  const [key, setKey] = useState("");
  const test = () => {
    setTesting(true);
    request(Method.brainsCheck, { id: brain.id })
      .then(onChanged)
      .catch(fail)
      .finally(() => setTesting(false));
  };
  const saveKey = () =>
    request(Method.brainsSetKey, { id: brain.id, key })
      .then(() => {
        setKey("");
        toast(t("brains.keySaved"));
        onChanged();
      })
      .catch(fail);
  return (
    <Sheet open onOpenChange={(o) => !o && onClose()} title={brain.name}>
      <Group>
        <Row
          icon="info"
          title={t("brains.status")}
          end={<Tag tone={healthTone(brain.health)}>{healthLabel(t, brain.health)}</Tag>}
        />
        <Row
          icon={brain.privacy === "local" ? "privacy" : "cloud"}
          title={brain.privacy === "local" ? t("brains.privateHere") : t("brains.cloudBrain")}
          subtitle={brain.free ?? undefined}
        />
      </Group>
      {brain.models.length > 0 && (
        <>
          <Section title={t("brains.modelsTitle")} />
          <Note>{brain.models.slice(0, 30).join(", ")}</Note>
        </>
      )}
      {apiKey && (
        <>
          <Section title={t("brains.apiKey")} />
          <Note>{brain.hasKey ? t("brains.keyStored") : t("brains.keyMissing")}</Note>
          <div className="k-brains__key">
            <TextField
              type="password"
              autoComplete="off"
              value={key}
              placeholder={t("brains.keyPlaceholder")}
              onChange={(e) => setKey(e.target.value)}
              aria-label={t("brains.apiKey")}
            />
            <Button disabled={!key.trim()} onClick={() => void saveKey()}>
              {t("brains.testAndSave")}
            </Button>
          </div>
        </>
      )}
      <div className="k-brains__sheet-actions">
        <Button icon="refresh" disabled={testing} onClick={test}>
          {testing ? t("brains.testing") : t("brains.test")}
        </Button>
        {(brain.kind === "cli" || brain.id === "openrouter") && (
          <Button onClick={() => void request(Method.brainsSignIn, { id: brain.id }).then(onChanged).catch(fail)}>
            {t("brains.signIn")}
          </Button>
        )}
        <Button
          variant="destructive"
          icon="delete"
          onClick={() =>
            void request(Method.brainsDisconnect, { id: brain.id })
              .then(() => {
                onChanged();
                onClose();
              })
              .catch(fail)
          }
        >
          {t("brains.remove")}
        </Button>
      </div>
    </Sheet>
  );
}

type AddWay = "openrouter" | "key" | "custom";

function AddBrain({ catalog, onClose, onAdded }: { catalog: Catalog; onClose: () => void; onAdded: () => void }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const fail = useFail();
  const toast = useToast();
  const [way, setWay] = useState<AddWay>("openrouter");
  const keyed = useMemo(() => catalog.brains.filter((b) => b.signIn === "apiKey"), [catalog]);
  const [provider, setProvider] = useState(keyed[0]?.id ?? "anthropic");
  const [key, setKey] = useState("");
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [local, setLocal] = useState(true);
  const [busy, setBusy] = useState(false);
  const done = (message: string) => {
    toast(message);
    onAdded();
    onClose();
  };
  const go = () => {
    setBusy(true);
    let work: Promise<unknown>;
    if (way === "openrouter") {
      work = request(Method.brainsSignIn, { id: "openrouter" }).then(() => done(t("brains.signedIn")));
    } else if (way === "key") {
      work = request(Method.brainsConnect, { id: provider })
        .then(() => request(Method.brainsSetKey, { id: provider, key }))
        .then(() => done(t("brains.keySaved")));
    } else {
      const id = `custom-${
        name
          .trim()
          .toLowerCase()
          .replace(/[^a-z0-9]+/g, "-") || "server"
      }`;
      work = request(Method.brainsConnect, { id, name: name.trim() || url, baseUrl: url.trim(), local })
        .then(() => (key.trim() ? request(Method.brainsSetKey, { id, key }) : undefined))
        .then(() => done(t("brains.added")));
    }
    work.catch(fail).finally(() => setBusy(false));
  };
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title={t("brains.add")}
      description={t("brains.addHint")}
      footer={
        <Button
          variant="primary"
          disabled={busy || (way === "key" && !key.trim()) || (way === "custom" && !url.trim())}
          onClick={go}
        >
          {busy ? t("brains.working") : way === "openrouter" ? t("brains.connectOpenRouter") : t("brains.testAndSave")}
        </Button>
      }
    >
      <RadioGroup<AddWay> value={way} onChange={setWay} label={t("brains.add")}>
        <Row
          lead={<Radio value="openrouter" label={t("brains.wayOpenRouter")} />}
          title={t("brains.wayOpenRouter")}
          subtitle={t("brains.wayOpenRouterHint")}
          end={<Pill tone="success">{t("brains.freeModels")}</Pill>}
        />
        <Row
          lead={<Radio value="key" label={t("brains.wayKey")} />}
          title={t("brains.wayKey")}
          subtitle={t("brains.wayKeyHint")}
        />
        <Row
          lead={<Radio value="custom" label={t("brains.wayCustom")} />}
          title={t("brains.wayCustom")}
          subtitle={t("brains.wayCustomHint")}
        />
      </RadioGroup>
      {way === "key" && (
        <div className="k-brains__form">
          <Select
            label={t("brains.provider")}
            value={provider}
            onChange={setProvider}
            items={keyed.map((b) => ({ value: b.id, label: b.name }))}
          />
          <TextField
            type="password"
            autoComplete="off"
            value={key}
            placeholder={t("brains.keyPlaceholder")}
            onChange={(e) => setKey(e.target.value)}
            aria-label={t("brains.apiKey")}
          />
          <Note>{t("brains.keyWriteOnly")}</Note>
        </div>
      )}
      {way === "custom" && (
        <div className="k-brains__form">
          <TextField
            value={name}
            placeholder={t("brains.customName")}
            onChange={(e) => setName(e.target.value)}
            aria-label={t("brains.customName")}
          />
          <TextField
            value={url}
            placeholder="http://127.0.0.1:8000/v1"
            onChange={(e) => setUrl(e.target.value)}
            aria-label={t("brains.customUrl")}
          />
          <Row
            icon="privacy"
            title={t("brains.customLocal")}
            end={<Switch checked={local} onChange={setLocal} label={t("brains.customLocal")} />}
          />
          <TextField
            type="password"
            autoComplete="off"
            value={key}
            placeholder={t("brains.keyOptional")}
            onChange={(e) => setKey(e.target.value)}
            aria-label={t("brains.apiKey")}
          />
        </div>
      )}
    </Dialog>
  );
}

function ProfileSheet({
  profile,
  brains,
  onClose,
  onSaved,
}: {
  profile: Profile;
  brains: BrainView[];
  onClose: () => void;
  onSaved: () => void;
}) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const fail = useFail();
  const [draft, setDraft] = useState<Profile>(profile);
  const primary = draft.primary?.provider ?? "";
  const models = brains.find((b) => b.id === primary)?.models ?? [];
  const save = () =>
    request(Method.brainsSaveProfile, draft)
      .then(() => {
        onSaved();
        onClose();
      })
      .catch(fail);
  return (
    <Sheet open onOpenChange={(o) => !o && onClose()} title={draft.name}>
      <div className="k-brains__form">
        <TextField
          value={draft.name}
          onChange={(e) => setDraft({ ...draft, name: e.target.value })}
          aria-label={t("brains.profileName")}
        />
        <Select
          label={t("brains.primary")}
          value={primary || "auto"}
          onChange={(v) => setDraft({ ...draft, primary: v === "auto" ? null : { provider: v, model: "" } })}
          items={[{ value: "auto", label: t("brains.auto") }, ...brains.map((b) => ({ value: b.id, label: b.name }))]}
        />
        {primary && models.length > 0 && (
          <Select
            label={t("brains.model")}
            value={draft.primary?.model || models[0]}
            onChange={(v) => setDraft({ ...draft, primary: { provider: primary, model: v } })}
            items={models.slice(0, 50).map((m) => ({ value: m, label: m }))}
          />
        )}
        <Select
          label={t("brains.privacy")}
          value={draft.privacy}
          onChange={(v) => setDraft({ ...draft, privacy: v })}
          items={[
            { value: "cloud", label: t("brains.privacyCloud") },
            { value: "localPreferred", label: t("brains.privacyLocalPreferred") },
            { value: "strictPrivate", label: t("brains.privacyStrict") },
          ]}
        />
        <Select
          label={t("brains.personality")}
          value={draft.persona ?? "inherit"}
          onChange={(v) => setDraft({ ...draft, persona: v === "inherit" ? null : v })}
          items={[
            { value: "inherit", label: t("brains.personaInherit") },
            { value: "calm", label: t("persona.calm") },
            { value: "friendly", label: t("persona.friendly") },
            { value: "witty", label: t("persona.witty") },
          ]}
        />
        <TextArea
          value={draft.systemPromptAddendum}
          placeholder={t("brains.addendum")}
          onChange={(e) => setDraft({ ...draft, systemPromptAddendum: e.target.value })}
          aria-label={t("brains.addendum")}
        />
        <TextField
          type="number"
          min={1000}
          value={draft.maxContext ?? ""}
          placeholder={t("brains.maxContext")}
          onChange={(e) => setDraft({ ...draft, maxContext: e.target.value ? Number(e.target.value) : null })}
          aria-label={t("brains.maxContext")}
        />
      </div>
      <div className="k-brains__sheet-actions">
        <Button variant="primary" onClick={() => void save()}>
          {t("brains.save")}
        </Button>
        <Button
          variant={profile.builtIn ? "secondary" : "destructive"}
          onClick={() =>
            void request(Method.brainsDeleteProfile, { id: profile.id })
              .then(() => {
                onSaved();
                onClose();
              })
              .catch(fail)
          }
        >
          {profile.builtIn ? t("brains.reset") : t("brains.remove")}
        </Button>
      </div>
    </Sheet>
  );
}

/** Settings → Context (CONV-30): what a conversation starts with, layer by layer. */
function ContextTab() {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const fail = useFail();
  const connected = link?.status === "connected";
  const [preview, setPreview] = useState<ContextPreview | null>(null);
  const [prefs, setPrefs] = useState<{ key: string; value: string }[]>([]);
  const [key, setKey] = useState("");
  const [value, setValue] = useState("");
  const [viewing, setViewing] = useState<string | null>(null);
  const load = useCallback(() => {
    if (!connected) return;
    void request<ContextPreview>(Method.brainsContext)
      .then(setPreview)
      .catch(() => {});
    void request<{ key: string; value: string }[]>(Method.memoryPreferences)
      .then(setPrefs)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  const number = new Intl.NumberFormat(i18n.language);
  if (!connected) return <Note>{t("brains.notConnected")}</Note>;
  if (!preview) return <Spinner label={t("brains.loading")} />;
  const start = preview.layers.filter((l) => l.id !== "turns").reduce((sum, l) => sum + l.tokens, 0);
  const colors: Record<string, string> = {
    system: "var(--text)",
    instructions: "var(--acc)",
    live: "#8E8E93",
    summary: "#32D74B",
    tools: "#FF9F0A",
  };
  const shown = preview.layers.filter((l) => l.id !== "turns");
  return (
    <div className="k-brains">
      <div className="k-context__tile">
        <b>{t("context.start", { tokens: number.format(start) })}</b>
        <span className="k-meta">
          {preview.brain
            ? t("context.budget", {
                brain: preview.brain,
                voice: number.format(preview.budget.voice),
                chat: number.format(preview.budget.chat),
              })
            : t("context.noBrain")}
        </span>
        <div className="k-context__bar" aria-hidden>
          {shown.map((l) => (
            <i key={l.id} style={{ width: `${(l.tokens / Math.max(1, start)) * 100}%`, background: colors[l.id] }} />
          ))}
        </div>
        <div className="k-context__legend">
          {shown.map((l) => (
            <span key={l.id}>
              <b style={{ background: colors[l.id] }} />
              {t(`context.layer.${l.id}`)} {number.format(l.tokens)}
            </span>
          ))}
        </div>
      </div>
      <Section title={t("context.always")} />
      <Group>
        {preview.layers
          .filter((l) => l.id === "system" || l.id === "instructions" || l.id === "live")
          .map((l) => (
            <Row
              key={l.id}
              icon={l.id === "system" ? "file" : l.id === "instructions" ? "user" : "compass"}
              title={t(`context.layer.${l.id}`)}
              subtitle={t(`context.hint.${l.id}`)}
              end={
                <>
                  <span className="k-meta">{t("context.tokensOf", { tokens: l.tokens, max: l.max ?? 0 })}</span>
                  <Button size="sm" variant="plain" onClick={() => setViewing(l.text ?? "")}>
                    {t("context.view")}
                  </Button>
                </>
              }
            />
          ))}
      </Group>
      <Section title={t("context.onlyWhenRelevant")} />
      <Group>
        <Row icon="memory" title={t("context.layer.summary")} subtitle={t("context.hint.summary")} />
        <Row
          icon="connector"
          title={t("context.layer.tools")}
          subtitle={t("context.hint.tools", { count: preview.layers.find((l) => l.id === "tools")?.count ?? 0 })}
        />
      </Group>
      <Section title={t("context.aboutMe")} />
      <Group>
        {prefs.map((p) => (
          <Row
            key={p.key}
            icon="user"
            title={p.key}
            subtitle={p.value}
            end={
              <IconButton
                size="sm"
                icon="delete"
                label={t("context.removePreference", { key: p.key })}
                onClick={() => void request(Method.memoryDeletePreference, { key: p.key }).then(load).catch(fail)}
              />
            }
          />
        ))}
      </Group>
      <div className="k-brains__key">
        <TextField
          value={key}
          placeholder={t("context.prefKey")}
          onChange={(e) => setKey(e.target.value)}
          aria-label={t("context.prefKey")}
        />
        <TextField
          value={value}
          placeholder={t("context.prefValue")}
          onChange={(e) => setValue(e.target.value)}
          aria-label={t("context.prefValue")}
        />
        <Button
          disabled={!key.trim() || !value.trim()}
          onClick={() =>
            void request(Method.memorySetPreference, { key, value })
              .then(() => {
                setKey("");
                setValue("");
                load();
              })
              .catch(fail)
          }
        >
          {t("context.addPreference")}
        </Button>
      </div>
      <Note>{t("context.note")}</Note>
      {viewing !== null && (
        <Sheet open onOpenChange={(o) => !o && setViewing(null)} title={t("context.view")}>
          <pre className="k-context__text">{viewing || t("context.empty")}</pre>
        </Sheet>
      )}
    </div>
  );
}
