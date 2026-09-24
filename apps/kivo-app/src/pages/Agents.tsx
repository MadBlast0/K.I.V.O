/**
 * Agents (UX-25): other AIs KIVO runs and talks to for the user.
 *
 * - Agents: sessions (KIVO's own over ACP and the visible terminals it started), "Open in
 *   terminal" for a session the agent can resume (CONV-13), the CLI agents found on this PC with
 *   "Start" (a folder and a mode; bypass asks first, CONV-14), and the desktop AI apps found in the
 *   installed apps (DISC-06).
 * - Workspaces (CONV-09/10/11): "About me" (global instructions), each remembered workspace with its
 *   instructions, the project's own agent files, "Export as AGENTS.md" and Forget. The runtime
 *   mirrors instructions to Markdown files and re-imports them when they are edited there.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { InstallSheet } from "../components/agents/InstallSheet";
import { PageHeader } from "../components/layout/Shell";
import {
  Button,
  Dialog,
  EmptyState,
  Group,
  Meta,
  Monogram,
  Note,
  PageTabs,
  Pill,
  Row,
  Section,
  Segmented,
  Select,
  Switch,
  Tag,
  TextArea,
  useToast,
} from "../components/ui";
import { Method, type AgentItem, type AgentsOverview, type WorkspaceItem } from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";

type Tab = "agents" | "workspaces";
/** The launch modes the Start dialog offers, as the app registry names them. */
export type LaunchMode = "default" | "accept-edits" | "plan" | "bypass";

const COLORS: Record<string, string> = {
  "claude-code": "#C96442",
  codex: "#10A37F",
  "gemini-cli": "#4285F4",
  "claude-desktop": "#D97757",
  chatgpt: "#10A37F",
  copilot: "#0078D4",
};

/** "Claude Code" → "CC". */
export function initials(name: string): string {
  const words = name.split(/[\s-]+/).filter(Boolean);
  const letters = words.length > 1 ? words.slice(0, 2).map((w) => w[0]) : [name.slice(0, 2)];
  return letters.join("").toUpperCase();
}

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function Agents({ initialTab = "agents" }: { initialTab?: Tab }) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>(initialTab);
  return (
    <>
      <PageHeader title={t("nav.agents")} subtitle={t("agents.subtitle")} />
      <PageTabs<Tab>
        label={t("nav.agents")}
        value={tab}
        onChange={setTab}
        tabs={[
          { value: "agents", label: t("agents.tabs.agents"), content: <AgentsTab /> },
          { value: "workspaces", label: t("agents.tabs.workspaces"), content: <WorkspacesTab /> },
        ]}
      />
    </>
  );
}

function useWorkspaces(): [WorkspaceItem[], string | null, () => void] {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [items, setItems] = useState<WorkspaceItem[]>([]);
  const [current, setCurrent] = useState<string | null>(null);
  const load = useCallback(() => {
    if (!connected) return;
    void request<{ workspaces: WorkspaceItem[]; current: string | null }>(Method.workspacesList)
      .then((r) => {
        setItems(r.workspaces);
        setCurrent(r.current);
      })
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  return [items, current, load];
}

function AgentsTab() {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [overview, setOverview] = useState<AgentsOverview | null>(null);
  const [starting, setStarting] = useState<AgentItem | null>(null);
  const [workspaces] = useWorkspaces();
  // Which agents may use KIVO's MCP server, and so the user's memory (CONV-24).
  const [sharing, setSharing] = useState<{ agents: string[]; sensitive: boolean }>({ agents: [], sensitive: false });
  useEffect(() => {
    if (!connected) return;
    void request<{ tools: Record<string, unknown> }>(Method.settingsGet)
      .then((s) => {
        const agents = s.tools["share-with-agents"];
        setSharing({
          agents: Array.isArray(agents) ? agents.filter((a): a is string => typeof a === "string") : [],
          sensitive: s.tools["share-sensitive"] === true,
        });
      })
      .catch(() => {});
  }, [connected, request]);
  const share = (params: { agent: string; on: boolean; sensitive?: boolean }) => {
    request<{ agents: string[]; sensitive: boolean }>(Method.mcpShare, params)
      .then(setSharing)
      .catch((e: unknown) => toast(message(e)));
  };

  const load = useCallback(() => {
    if (!connected) return;
    void request<AgentsOverview>(Method.agentsOverview)
      .then(setOverview)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  // Node.js, Git and Ollama, with their versions (DIST-14).
  const [tools, setTools] = useState<{ id: string; name: string; version: string | null }[]>([]);
  const [installing, setInstalling] = useState<{ id: string; agent: boolean } | null>(null);
  const loadTools = useCallback(() => {
    if (!connected) return;
    void request<{ id: string; name: string; version: string | null }[] | null>(Method.installsTools)
      .then((list) => setTools(list ?? []))
      .catch(() => {});
  }, [connected, request]);
  useEffect(loadTools, [loadTools]);
  // A turn or a task may have started or ended a session.
  useRuntimeEvents((event) => {
    if (event.group === "task" || (event.group === "turn" && event.event.type === "completed")) load();
  });

  if (!connected) return <Note>{t("voice.notConnected")}</Note>;
  if (!overview) return null;

  const time = new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" });
  const openInTerminal = (session: string) => {
    request<{ title: string }>(Method.agentsOpenInTerminal, { session })
      .then((r) => toast(t("agents.openedIn", { title: r.title })))
      .catch((e: unknown) => toast(message(e)));
  };
  const installed = overview.cli.filter((a) => a.installed);
  const missing = overview.cli.filter((a) => !a.installed);

  return (
    <>
      <Section title={t("agents.sessions")} />
      {overview.sessions.length === 0 ? (
        <Note>{t("agents.noSessions")}</Note>
      ) : (
        <Group>
          {overview.sessions.map((s) => (
            <Row
              key={s.id}
              lead={<Monogram text={initials(s.agent)} color={COLORS[s.agent] ?? "#555"} />}
              title={`${s.agent} · ${s.workspace}`}
              subtitle={[t(`agents.kind.${s.kind}`), time.format(new Date(s.lastUsed))].join(" · ")}
              end={
                <>
                  <Tag tone={s.running ? "accent" : "neutral"}>
                    {s.running ? t("agents.working") : t("agents.idle")}
                  </Tag>
                  {s.kind === "acp" && s.resumable && (
                    <Button size="sm" icon="terminal" onClick={() => openInTerminal(s.id)}>
                      {t("agents.openInTerminal")}
                    </Button>
                  )}
                </>
              }
            />
          ))}
        </Group>
      )}
      <Note>{t("agents.draftHint")}</Note>

      <Section title={t("agents.cli")} />
      {installed.length === 0 ? (
        <EmptyState icon="agent" title={t("agents.noneInstalled")}>
          {t("agents.noneInstalledDetail")}
        </EmptyState>
      ) : (
        <Group>
          {installed.map((a) => (
            <Row
              key={a.id}
              lead={<Monogram text={initials(a.name)} color={COLORS[a.id] ?? "#555"} />}
              title={a.name}
              subtitle={[
                a.version,
                a.signedIn === false ? t("agents.needsSignIn") : a.signedIn ? t("agents.signedIn") : undefined,
              ]
                .filter(Boolean)
                .join(" · ")}
              end={
                a.terminal ? (
                  <Button size="sm" icon="play" onClick={() => setStarting(a)}>
                    {t("agents.start")}
                  </Button>
                ) : (
                  <Meta>{t("agents.backgroundOnly")}</Meta>
                )
              }
            />
          ))}
        </Group>
      )}
      {missing.length > 0 && (
        <>
          <Section title={t("agents.notInstalledTitle")} />
          <Group>
            {missing.map((a) => (
              <Row
                key={a.id}
                lead={<Monogram text={initials(a.name)} color={COLORS[a.id] ?? "#555"} />}
                title={a.name}
                subtitle={t("agents.notInstalledHint")}
                end={
                  <Button size="sm" icon="download" onClick={() => setInstalling({ id: a.id, agent: true })}>
                    {t("install.install")}
                  </Button>
                }
              />
            ))}
          </Group>
        </>
      )}

      <Section title={t("agents.tools")} />
      <Group>
        {tools.map((d) => (
          <Row
            key={d.id}
            icon="terminal"
            title={d.name}
            subtitle={d.version ? t("agents.toolVersion", { version: d.version }) : t("agents.toolMissing")}
            end={
              d.version ? (
                <Tag tone="success">{t("agents.found")}</Tag>
              ) : (
                <Button size="sm" icon="download" onClick={() => setInstalling({ id: d.id, agent: false })}>
                  {t("install.install")}
                </Button>
              )
            }
          />
        ))}
      </Group>
      {installing && (
        <InstallSheet
          id={installing.id}
          agent={installing.agent}
          onClose={() => setInstalling(null)}
          onDone={() => {
            load();
            loadTools();
          }}
        />
      )}

      <Section title={t("agents.desktop")} />
      {overview.desktop.length === 0 ? (
        <Note>{t("agents.noDesktop")}</Note>
      ) : (
        <Group>
          {overview.desktop.map((d) => (
            <Row
              key={d.id}
              lead={<Monogram text={initials(d.name)} color={COLORS[d.id] ?? "#555"} />}
              title={d.name}
              subtitle={t("agents.desktopDetail")}
              end={<Tag tone="success">{t("agents.found")}</Tag>}
            />
          ))}
        </Group>
      )}

      <Section title={t("agents.safety")} />
      <Group>
        <Row
          icon="permissions"
          title={t("agents.bypassRisk")}
          subtitle={t("agents.bypassRiskDetail")}
          end={<Pill tone="danger">{t("risk.high")}</Pill>}
        />
        <Row icon="send" title={t("agents.reviewPrompts")} subtitle={t("agents.reviewPromptsDetail")} />
      </Group>

      <Section title={t("agents.memory")} />
      <Group>
        {installed.map((a) => (
          <Row
            key={a.id}
            icon="memory"
            title={t("agents.shareWith", { name: a.name })}
            subtitle={t("agents.shareDetail")}
            end={
              <Switch
                label={t("agents.shareWith", { name: a.name })}
                checked={sharing.agents.includes(a.id)}
                onChange={(on) => share({ agent: a.id, on })}
              />
            }
          />
        ))}
        <Row
          icon="privacy"
          title={t("agents.shareSensitive")}
          subtitle={t("agents.shareSensitiveDetail")}
          end={
            <Switch
              label={t("agents.shareSensitive")}
              checked={sharing.sensitive}
              disabled={sharing.agents.length === 0}
              onChange={(sensitive) => share({ agent: sharing.agents[0] ?? "", on: true, sensitive })}
            />
          }
        />
      </Group>

      {starting && (
        <StartDialog
          agent={starting}
          workspaces={workspaces}
          onClose={() => setStarting(null)}
          onStarted={() => {
            setStarting(null);
            load();
          }}
        />
      )}
    </>
  );
}

function StartDialog({
  agent,
  workspaces,
  onClose,
  onStarted,
}: {
  agent: AgentItem;
  workspaces: WorkspaceItem[];
  onClose: () => void;
  onStarted: () => void;
}) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [folder, setFolder] = useState(workspaces[0]?.path ?? "");
  const [mode, setMode] = useState<LaunchMode>("default");
  const start = () => {
    // The runtime runs it as a request of its own: the permission engine decides, and a bypass
    // launch waits for the user's yes in the Island (CONV-14).
    request(Method.agentsStart, { agent: agent.id, folder, mode })
      .then(onStarted)
      .catch((e: unknown) => toast(message(e)));
  };
  return (
    <Dialog
      open
      onOpenChange={(open) => !open && onClose()}
      title={t("agents.startNamed", { name: agent.name })}
      description={t("agents.startDetail")}
      footer={
        <Button variant="primary" icon="play" disabled={folder === ""} onClick={start}>
          {t("agents.start")}
        </Button>
      }
    >
      {workspaces.length === 0 ? (
        <Note>{t("agents.noWorkspaces")}</Note>
      ) : (
        <Select<string>
          label={t("agents.folder")}
          icon="folder"
          value={folder}
          onChange={setFolder}
          items={workspaces.map((w) => ({ value: w.path, label: w.name }))}
        />
      )}
      <div className="k-dialog__row">
        <Segmented<LaunchMode>
          label={t("agents.mode")}
          value={mode}
          onChange={setMode}
          options={[
            { value: "default", label: t("agents.modes.default") },
            { value: "plan", label: t("agents.modes.plan") },
            { value: "accept-edits", label: t("agents.modes.accept-edits") },
            { value: "bypass", label: t("agents.modes.bypass") },
          ]}
        />
      </div>
      {mode === "bypass" && <Note>{t("agents.bypassNote")}</Note>}
    </Dialog>
  );
}

/* ───────────────────────────── Workspaces ───────────────────────────── */

function WorkspacesTab() {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const [workspaces, current, load] = useWorkspaces();
  if (link?.status !== "connected") return <Note>{t("voice.notConnected")}</Note>;
  const time = new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium" });

  const forget = (w: WorkspaceItem) => {
    request(Method.workspacesForget, { id: w.id })
      .then(load)
      .catch((e: unknown) => toast(message(e)));
  };
  const exportAgents = (w: WorkspaceItem) => {
    request<{ file: string }>(Method.workspacesExportAgentsMd, { id: w.id })
      .then((r) => toast(t("workspaces.exported", { file: r.file })))
      .catch((e: unknown) => toast(message(e)));
  };

  return (
    <>
      <Section title={t("workspaces.aboutMe")} />
      <Instructions scope="global" placeholder={t("workspaces.aboutMePlaceholder")} />
      <Section title={t("workspaces.title")} />
      {workspaces.length === 0 ? (
        <EmptyState icon="folder" title={t("workspaces.none")}>
          {t("workspaces.noneDetail")}
        </EmptyState>
      ) : (
        workspaces.map((w) => (
          <div key={w.id} className="k-workspace">
            <Group>
              <Row
                icon="folder"
                title={w.name}
                subtitle={[w.path, t("workspaces.lastUsed", { date: time.format(new Date(w.lastUsed)) })].join(" · ")}
                end={
                  <>
                    {w.id === current && <Tag tone="accent">{t("workspaces.current")}</Tag>}
                    {w.agentFiles.map((f) => (
                      <Pill key={f}>{f}</Pill>
                    ))}
                    <Button size="sm" onClick={() => exportAgents(w)}>
                      {t("workspaces.export")}
                    </Button>
                    <Button size="sm" variant="plain" icon="delete" onClick={() => forget(w)}>
                      {t("workspaces.forget")}
                    </Button>
                  </>
                }
              />
            </Group>
            <Instructions
              scope={`workspace:${w.id}`}
              placeholder={t("workspaces.instructionsPlaceholder", { name: w.name })}
            />
          </div>
        ))
      )}
      <Note>{t("workspaces.note")}</Note>
    </>
  );
}

/** One set of instructions, saved when the field loses focus. */
function Instructions({ scope, placeholder }: { scope: string; placeholder: string }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [text, setText] = useState<string | null>(null);
  const [saved, setSaved] = useState("");
  useEffect(() => {
    void request<{ text: string }>(Method.instructionsGet, { scope })
      .then((r) => {
        setText(r.text);
        setSaved(r.text);
      })
      .catch(() => setText(""));
  }, [request, scope]);
  if (text === null) return null;
  const save = () => {
    if (text === saved) return;
    request(Method.instructionsSet, { scope, text })
      .then(() => {
        setSaved(text);
        toast(t("workspaces.instructionsSaved"));
      })
      .catch((e: unknown) => toast(message(e)));
  };
  return (
    <TextArea
      aria-label={placeholder}
      placeholder={placeholder}
      rows={4}
      value={text}
      onChange={(e) => setText(e.target.value)}
      onBlur={save}
    />
  );
}
