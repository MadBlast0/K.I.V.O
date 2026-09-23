/**
 * Extensions (UX-27): Connectors · MCP servers · Plugins · Skills.
 *
 * - Connectors (INT-03, DISC-08): the directory — connected ones, ones ready on this PC (a switch),
 *   built-in ones, and ones to connect with a browser sign-in; a custom remote MCP by its URL.
 * - MCP servers (TOOL-34/35/36, DISC-09): other apps' setups to import (the originals are never
 *   changed), and each server with its tools. A new tool, or one whose description changed after
 *   it was approved, waits for review; each tool has its own switch and risk.
 * - Plugins: they arrive after 1.0 (INTEGRATIONS §3).
 * - Skills (CONV-32, DISC-10): KIVO's own and other apps', reviewed before they're on.
 *
 * The page reloads when the runtime says a section changed (DISCOVERY §3); Refresh re-runs a
 * section's detectors.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import {
  Button,
  Checkbox,
  Dialog,
  EmptyState,
  Group,
  IconButton,
  Meta,
  Monogram,
  Note,
  PageTabs,
  Pill,
  Row,
  SearchField,
  Section,
  Segmented,
  Select,
  Spinner,
  Switch,
  Tag,
  TextField,
  useToast,
  type Tone,
} from "../components/ui";
import {
  Method,
  type ConnectorView,
  type McpFoundView,
  type McpServerView,
  type McpToolView,
  type Risk,
  type SkillView,
} from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";
import { initials } from "./Agents";
import { ago } from "../lib/ago";

export const EXTENSION_TABS = ["connectors", "mcp", "plugins", "skills"] as const;
type Tab = (typeof EXTENSION_TABS)[number];

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** Loads with `load`, again when the runtime says `section` changed. */
function useSection<T>(method: Method, section: string, fallback: T): [T, () => void] {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [value, setValue] = useState<T>(fallback);
  const load = useCallback(() => {
    if (!connected) return;
    void request<T>(method)
      .then(setValue)
      .catch(() => {});
  }, [connected, method, request]);
  useEffect(load, [load]);
  useRuntimeEvents((event) => {
    if (event.group === "system" && event.event.type === "discoveryChanged" && event.event.section === section) {
      load();
    }
  });
  return [value, load];
}

function useAct() {
  const { request } = useRuntime();
  const toast = useToast();
  return useCallback(
    (method: Method, params: Record<string, unknown>, then?: () => void) =>
      request(method, params)
        .then(() => then?.())
        .catch((e: unknown) => toast(message(e))),
    [request, toast],
  );
}

export function Extensions({ initialTab = "connectors" }: { initialTab?: Tab }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>(initialTab);
  const { link } = useRuntime();
  return (
    <>
      <PageHeader title={t("nav.extensions")} subtitle={t("extensions.subtitle")} />
      {link?.status !== "connected" ? (
        <Note>{t("voice.notConnected")}</Note>
      ) : (
        <PageTabs<Tab>
          label={t("nav.extensions")}
          value={tab}
          onChange={setTab}
          tabs={[
            { value: "connectors", label: t("extensions.tabs.connectors"), content: <ConnectorsTab /> },
            { value: "mcp", label: t("extensions.tabs.mcp"), content: <McpTab /> },
            { value: "plugins", label: t("extensions.tabs.plugins"), content: <PluginsTab /> },
            { value: "skills", label: t("extensions.tabs.skills"), content: <SkillsTab /> },
          ]}
        />
      )}
    </>
  );
}

function RefreshButton({ section }: { section: string }) {
  const { t } = useTranslation();
  const act = useAct();
  const [busy, setBusy] = useState(false);
  return (
    <Button
      size="sm"
      variant="plain"
      icon="refresh"
      disabled={busy}
      onClick={() => {
        setBusy(true);
        void act(Method.extensionsRefresh, { section }).finally(() => setBusy(false));
      }}
    >
      {t("extensions.refresh")}
    </Button>
  );
}

const COLORS: Record<string, string> = {
  github: "#24292F",
  "github-cli": "#24292F",
  notion: "#191919",
  linear: "#5E6AD2",
  atlassian: "#0052CC",
  stripe: "#635BFF",
  spotify: "#1DB954",
  vscode: "#0078D4",
};

/* ───────────────────────────── Connectors ───────────────────────────── */

function ConnectorsTab() {
  const { t, i18n } = useTranslation();
  const act = useAct();
  const [list, reload] = useSection<ConnectorView[]>(Method.connectorsList, "connectors", []);
  const [query, setQuery] = useState("");
  const [custom, setCustom] = useState(false);
  const shown = list.filter(
    (c) => !query.trim() || `${c.name} ${c.description}`.toLowerCase().includes(query.trim().toLowerCase()),
  );
  const connect = (id: string) => void act(Method.connectorsConnect, { id }, reload);
  const disconnect = (id: string) => void act(Method.connectorsDisconnect, { id }, reload);
  const lead = (c: ConnectorView) =>
    c.kind === "builtIn" ? undefined : <Monogram text={initials(c.name)} color={COLORS[c.id] ?? "#555"} />;
  const connected = shown.filter((c) => c.kind === "remote" && (c.state === "connected" || c.state === "error"));
  const ready = shown.filter((c) => c.kind === "local" && (c.state === "ready" || c.state === "off"));
  const builtIn = shown.filter((c) => c.kind === "builtIn");
  const available = shown.filter(
    (c) =>
      (c.kind === "remote" && (c.state === "available" || c.state === "connecting")) ||
      (c.kind === "local" && c.state === "available"),
  );
  return (
    <>
      <div className="k-toolbar">
        <SearchField
          aria-label={t("extensions.searchConnectors")}
          placeholder={t("extensions.searchConnectors")}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        <Button icon="link" onClick={() => setCustom(true)}>
          {t("extensions.addCustom")}
        </Button>
      </div>
      {connected.length > 0 && (
        <>
          <Section title={t("extensions.connected")} aside={<Meta>{connected.length}</Meta>} />
          <Group>
            {connected.map((c) => (
              <Row
                key={c.id}
                lead={lead(c)}
                title={c.name}
                subtitle={
                  c.state === "error"
                    ? (c.detail ?? "")
                    : c.lastUsed === null
                      ? t("extensions.connectorTools", { count: c.tools })
                      : t("extensions.toolsUsed", { count: c.tools, when: ago(c.lastUsed, i18n.language) })
                }
                end={
                  <>
                    {c.state === "error" ? (
                      <Tag tone="danger">{t("extensions.state.error")}</Tag>
                    ) : (
                      <Tag tone="success">{t("extensions.state.connected")}</Tag>
                    )}
                    <Button size="sm" onClick={() => disconnect(c.server ?? c.id)}>
                      {t("extensions.disconnect")}
                    </Button>
                  </>
                }
              />
            ))}
          </Group>
        </>
      )}
      <Section title={t("extensions.readyToUse")} aside={<RefreshButton section="connectors" />} />
      {ready.length === 0 ? (
        <Note>{t("extensions.noneReady")}</Note>
      ) : (
        <Group>
          {ready.map((c) => (
            <Row
              key={c.id}
              lead={lead(c)}
              title={c.name}
              subtitle={c.detail ?? c.description}
              end={
                <Switch
                  label={t("extensions.useConnector", { name: c.name })}
                  checked={c.state === "ready"}
                  onChange={(on) => (on ? connect(c.id) : disconnect(c.id))}
                />
              }
            />
          ))}
        </Group>
      )}
      <Section title={t("extensions.builtIn")} aside={<Meta>{t("extensions.noSignIn")}</Meta>} />
      <Group>
        {builtIn.map((c) => (
          <Row
            key={c.id}
            icon="music"
            title={c.name}
            subtitle={c.description}
            end={<Tag tone="success">{t("extensions.state.on")}</Tag>}
          />
        ))}
      </Group>
      <Section title={t("extensions.available")} />
      <Group>
        {available.map((c) => (
          <Row
            key={c.id}
            lead={lead(c)}
            title={c.name}
            subtitle={`${c.description} · ${c.access}`}
            end={
              <>
                {c.badges.includes("sensitive") && <Pill tone="warning">{t("extensions.badge.sensitive")}</Pill>}
                {c.kind === "local" ? (
                  <Meta>{t("extensions.notFound")}</Meta>
                ) : c.state === "connecting" ? (
                  <Spinner label={t("extensions.signingIn")} />
                ) : (
                  <Button size="sm" onClick={() => connect(c.id)}>
                    {t("extensions.connect")}
                  </Button>
                )}
              </>
            }
          />
        ))}
      </Group>
      <Note>{t("extensions.connectorsNote")}</Note>
      {custom && <CustomConnector onClose={() => setCustom(false)} onDone={reload} />}
    </>
  );
}

function CustomConnector({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const { t } = useTranslation();
  const act = useAct();
  const [url, setUrl] = useState("");
  const [name, setName] = useState("");
  return (
    <Dialog
      open
      onOpenChange={(open) => !open && onClose()}
      title={t("extensions.customTitle")}
      description={t("extensions.customDetail")}
      footer={
        <Button
          variant="primary"
          disabled={!url.trim()}
          onClick={() =>
            void act(Method.connectorsConnect, { url: url.trim(), name: name.trim() }, () => {
              onDone();
              onClose();
            })
          }
        >
          {t("extensions.connect")}
        </Button>
      }
    >
      <TextField
        icon="link"
        aria-label={t("extensions.serverUrl")}
        placeholder="https://"
        value={url}
        onChange={(e) => setUrl(e.target.value)}
      />
      <div className="k-dialog__row">
        <TextField
          aria-label={t("extensions.serverName")}
          placeholder={t("extensions.serverName")}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </div>
      <Note>{t("extensions.customNote")}</Note>
    </Dialog>
  );
}

/* ───────────────────────────── MCP servers ───────────────────────────── */

const RISKS: ReadonlyArray<Risk> = ["safe", "low", "medium", "high"];
const STATUS_TONE: Record<string, Tone> = {
  running: "success",
  needsReview: "warning",
  error: "danger",
  connecting: "accent",
  stopped: "neutral",
};

function McpTab() {
  const { t } = useTranslation();
  const act = useAct();
  const [servers, reload] = useSection<McpServerView[]>(Method.mcpList, "mcp", []);
  const [found, reloadFound] = useSection<McpFoundView[]>(Method.mcpFound, "mcp", []);
  const [open, setOpen] = useState<string | null>(null);
  const [reviewing, setReviewing] = useState<McpServerView | null>(null);
  const [adding, setAdding] = useState(false);
  const both = () => {
    reload();
    reloadFound();
  };
  const importable = found.filter((f) => f.new.length > 0);
  return (
    <>
      <div className="k-toolbar">
        <span />
        <Button variant="primary" icon="add" onClick={() => setAdding(true)}>
          {t("extensions.addServer")}
        </Button>
      </div>
      <Section title={t("extensions.foundInApps")} aside={<RefreshButton section="mcp" />} />
      {importable.length === 0 ? (
        <Note>{t("extensions.nothingToImport")}</Note>
      ) : (
        <Group>
          {importable.map((f) => (
            <Row
              key={f.file}
              lead={<Monogram text={initials(f.name)} color="#6B6B6B" />}
              title={f.name}
              subtitle={t("extensions.foundServers", { count: f.new.length, list: f.new.join(", ") })}
              end={
                <Button size="sm" variant="primary" onClick={() => void act(Method.mcpImport, { file: f.file }, both)}>
                  {t("extensions.import")}
                </Button>
              }
            />
          ))}
        </Group>
      )}
      <Note>{t("extensions.importNote")}</Note>
      <Section title={t("extensions.servers")} aside={<Meta>{servers.length}</Meta>} />
      {servers.length === 0 ? (
        <EmptyState icon="server" title={t("extensions.noServers")}>
          {t("extensions.noServersDetail")}
        </EmptyState>
      ) : (
        <Group>
          {servers.map((s) => (
            <div key={s.id} className="k-task">
              <Row
                icon="server"
                title={s.name}
                subtitle={[
                  s.kind === "remote" ? t("extensions.remote") : t("extensions.local"),
                  t("extensions.toolCount", { count: s.tools.length }),
                  s.source ? t("extensions.from", { source: s.source }) : undefined,
                  s.error ?? undefined,
                ]
                  .filter(Boolean)
                  .join(" · ")}
                end={
                  <>
                    <Tag tone={STATUS_TONE[s.status] ?? "neutral"}>
                      {t(`extensions.status.${s.status}`, { defaultValue: s.status })}
                    </Tag>
                    {s.status === "needsReview" && (
                      <Button size="sm" variant="primary" onClick={() => setReviewing(s)}>
                        {t("extensions.review")}
                      </Button>
                    )}
                    <Button
                      size="sm"
                      variant="plain"
                      aria-expanded={open === s.id}
                      onClick={() => setOpen(open === s.id ? null : s.id)}
                    >
                      {open === s.id ? t("extensions.hideTools") : t("extensions.showTools")}
                    </Button>
                    <Switch
                      label={t("extensions.serverOn", { name: s.name })}
                      checked={s.enabled}
                      onChange={(on) => void act(Method.mcpEnable, { id: s.id, on }, reload)}
                    />
                    <IconButton
                      icon="delete"
                      size="sm"
                      variant="plain"
                      label={t("extensions.removeServer", { name: s.name })}
                      onClick={() => void act(Method.mcpRemove, { id: s.id }, both)}
                    />
                  </>
                }
              />
              {open === s.id && <ServerTools server={s} onChange={reload} />}
            </div>
          ))}
        </Group>
      )}
      <Note>{t("extensions.riskNote")}</Note>
      {reviewing && (
        <Review
          server={reviewing}
          onClose={() => setReviewing(null)}
          onDone={() => {
            setReviewing(null);
            reload();
          }}
        />
      )}
      {adding && <AddServer onClose={() => setAdding(false)} onDone={reload} />}
    </>
  );
}

function ServerTools({ server, onChange }: { server: McpServerView; onChange: () => void }) {
  const { t } = useTranslation();
  const act = useAct();
  return (
    <ul className="k-task__steps k-mcp-tools">
      {server.tools.map((tool) => (
        <li key={tool.name} className="k-task__step">
          <code>{tool.name}</code>
          <Tag tone={tool.state === "approved" ? "neutral" : "warning"}>{t(`extensions.toolState.${tool.state}`)}</Tag>
          {tool.state === "approved" && (
            <>
              <Select<Risk>
                label={t("extensions.riskOf", { tool: tool.name })}
                value={tool.risk}
                onChange={(risk) => void act(Method.mcpSetTool, { id: server.id, tool: tool.name, risk }, onChange)}
                items={RISKS.map((r) => ({ value: r, label: t(`risk.${r}`) }))}
              />
              <Switch
                label={t("extensions.toolOn", { tool: tool.name })}
                checked={tool.enabled}
                onChange={(enabled) =>
                  void act(Method.mcpSetTool, { id: server.id, tool: tool.name, enabled }, onChange)
                }
              />
            </>
          )}
        </li>
      ))}
    </ul>
  );
}

/** Reviewing a server's new or changed tools (TOOL-36): what each one says about itself, then on or off. */
function Review({ server, onClose, onDone }: { server: McpServerView; onClose: () => void; onDone: () => void }) {
  const { t } = useTranslation();
  const act = useAct();
  const [on, setOn] = useState<string[]>(
    server.tools.filter((x) => x.state === "approved" && x.enabled).map((x) => x.name),
  );
  const toggle = (tool: McpToolView, value: boolean) =>
    setOn((cur) => (value ? [...cur, tool.name] : cur.filter((n) => n !== tool.name)));
  return (
    <Dialog
      open
      onOpenChange={(open) => !open && onClose()}
      title={t("extensions.reviewTitle", { name: server.name })}
      description={t("extensions.reviewDetail")}
      footer={
        <Button variant="primary" onClick={() => void act(Method.mcpApprove, { id: server.id, on }, onDone)}>
          {t("extensions.approve")}
        </Button>
      }
    >
      <div className="k-review">
        {server.tools.map((tool) => (
          <div key={tool.name} className="k-review__tool" data-state={tool.state}>
            <Checkbox checked={on.includes(tool.name)} onChange={(v) => toggle(tool, v)}>
              <code>{tool.name}</code>
            </Checkbox>
            {tool.state !== "approved" && <Tag tone="warning">{t(`extensions.toolState.${tool.state}`)}</Tag>}
            <p className="k-review__text">{tool.description || t("extensions.noDescription")}</p>
          </div>
        ))}
      </div>
    </Dialog>
  );
}

type AddKind = "remote" | "local";

function AddServer({ onClose, onDone }: { onClose: () => void; onDone: () => void }) {
  const { t } = useTranslation();
  const act = useAct();
  const [kind, setKind] = useState<AddKind>("remote");
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const ready = name.trim() !== "" && (kind === "remote" ? url.trim() !== "" : command.trim() !== "");
  const add = () => {
    const params =
      kind === "remote"
        ? { name: name.trim(), url: url.trim() }
        : { name: name.trim(), command: command.trim(), args: args.trim() ? args.trim().split(/\s+/) : [] };
    void act(Method.mcpAdd, params, () => {
      onDone();
      onClose();
    });
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => !open && onClose()}
      title={t("extensions.addServer")}
      description={t("extensions.addServerDetail")}
      footer={
        <Button variant="primary" disabled={!ready} onClick={add}>
          {t("extensions.add")}
        </Button>
      }
    >
      <Segmented<AddKind>
        label={t("extensions.serverKind")}
        value={kind}
        onChange={setKind}
        options={[
          { value: "remote", label: t("extensions.kindRemote") },
          { value: "local", label: t("extensions.kindLocal") },
        ]}
      />
      <div className="k-dialog__row">
        <TextField
          aria-label={t("extensions.serverName")}
          placeholder={t("extensions.serverName")}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
      </div>
      {kind === "remote" ? (
        <div className="k-dialog__row">
          <TextField
            icon="link"
            aria-label={t("extensions.serverUrl")}
            placeholder="https://"
            value={url}
            onChange={(e) => setUrl(e.target.value)}
          />
        </div>
      ) : (
        <>
          <div className="k-dialog__row">
            <TextField
              icon="terminal"
              aria-label={t("extensions.program")}
              placeholder="npx"
              value={command}
              onChange={(e) => setCommand(e.target.value)}
            />
          </div>
          <div className="k-dialog__row">
            <TextField
              aria-label={t("extensions.arguments")}
              placeholder="-y @modelcontextprotocol/server-filesystem D:\work"
              value={args}
              onChange={(e) => setArgs(e.target.value)}
            />
          </div>
        </>
      )}
      <Note>{t("extensions.trustNote")}</Note>
    </Dialog>
  );
}

/* ───────────────────────────── Plugins ───────────────────────────── */

function PluginsTab() {
  const { t } = useTranslation();
  return (
    <EmptyState icon="plugin" title={t("extensions.pluginsLater")}>
      {t("extensions.pluginsLaterDetail")}
    </EmptyState>
  );
}

/* ───────────────────────────── Skills ───────────────────────────── */

function SkillsTab() {
  const { t } = useTranslation();
  const act = useAct();
  const { request } = useRuntime();
  const toast = useToast();
  const [skills, reload] = useSection<SkillView[]>(Method.skillsList, "skills", []);
  const [path, setPath] = useState("");
  const [reviewing, setReviewing] = useState<{ skill: SkillView; text: string; files: string[] } | null>(null);
  const installed = skills.filter((s) => s.reviewed);
  const waiting = skills.filter((s) => !s.reviewed);
  const review = (skill: SkillView) => {
    request<{ text: string; files: string[] }>(Method.skillsRead, { id: skill.id })
      .then((r) => setReviewing({ skill, ...r }))
      .catch((e: unknown) => toast(message(e)));
  };
  const source = (s: SkillView) =>
    t(`extensions.skillSource.${s.source.split(":")[0] ?? ""}`, {
      defaultValue: s.source,
      name: s.source.split(":")[1] ?? "",
    });
  return (
    <>
      <div className="k-toolbar">
        <TextField
          icon="folder"
          aria-label={t("extensions.skillPath")}
          placeholder={t("extensions.skillPath")}
          value={path}
          onChange={(e) => setPath(e.target.value)}
        />
        <Button
          icon="add"
          disabled={!path.trim()}
          onClick={() =>
            void act(Method.skillsImport, { path: path.trim() }, () => {
              setPath("");
              reload();
            })
          }
        >
          {t("extensions.addSkill")}
        </Button>
      </div>
      <Section title={t("extensions.installedSkills")} aside={<RefreshButton section="skills" />} />
      {installed.length === 0 ? (
        <Note>{t("extensions.noSkills")}</Note>
      ) : (
        <Group>
          {installed.map((s) => (
            <Row
              key={s.id}
              icon="skill"
              title={s.name}
              subtitle={`${s.description} · ${source(s)}`}
              end={
                <>
                  {s.scripts && <Pill tone="warning">{t("extensions.runsScripts")}</Pill>}
                  <Meta>{t("extensions.tokens", { count: s.tokens })}</Meta>
                  <Switch
                    label={t("extensions.skillOn", { name: s.name })}
                    checked={s.enabled}
                    onChange={(on) => void act(Method.skillsEnable, { id: s.id, on }, reload)}
                  />
                  <IconButton
                    icon="delete"
                    size="sm"
                    variant="plain"
                    label={t("extensions.removeSkill", { name: s.name })}
                    onClick={() => void act(Method.skillsRemove, { id: s.id }, reload)}
                  />
                </>
              }
            />
          ))}
        </Group>
      )}
      {waiting.length > 0 && (
        <>
          <Section title={t("extensions.waitingReview")} aside={<Meta>{waiting.length}</Meta>} />
          <Group>
            {waiting.map((s) => (
              <Row
                key={s.id}
                icon="skill"
                title={s.name}
                subtitle={`${s.description} · ${source(s)}`}
                end={
                  <>
                    {s.source === "import" && <Pill tone="warning">{t("extensions.untrusted")}</Pill>}
                    <Button size="sm" onClick={() => review(s)}>
                      {t("extensions.review")}
                    </Button>
                  </>
                }
              />
            ))}
          </Group>
        </>
      )}
      <Note>{t("extensions.skillsNote")}</Note>
      {reviewing && (
        <Dialog
          open
          onOpenChange={(open) => !open && setReviewing(null)}
          title={t("extensions.reviewSkill", { name: reviewing.skill.name })}
          description={t("extensions.reviewSkillDetail")}
          footer={
            <Button
              variant="primary"
              onClick={() =>
                void act(Method.skillsEnable, { id: reviewing.skill.id, on: true }, () => {
                  setReviewing(null);
                  reload();
                })
              }
            >
              {t("extensions.turnOn")}
            </Button>
          }
        >
          <pre className="k-review__skill">{reviewing.text}</pre>
          <Section title={t("extensions.files")} />
          <ul className="k-review__files">
            {reviewing.files.map((f) => (
              <li key={f}>
                <code>{f}</code>
              </li>
            ))}
          </ul>
        </Dialog>
      )}
    </>
  );
}
