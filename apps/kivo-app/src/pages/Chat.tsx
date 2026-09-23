/**
 * Chat (UX-21, CONVERSATION §0–1): open-ended conversations with any brain. Threads on the left
 * (pinned, recent, search, voice threads too); the conversation with the brain chip and its
 * reason, the context meter and a cost estimate; the live answer, the tool steps and any decision
 * waiting, answered right here; a composer with the brain switcher, the permission-mode picker
 * (SEC-04), attachments and Stop. The thread's menu has rename, pin, continue by voice, branch,
 * export (Markdown or JSON, to Downloads), "Compact now" and delete (CONV-03); an answer can be
 * branched from, and "That's not what I meant" reports a misroute (BRAIN-06).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Alert,
  Button,
  Dialog,
  DropdownMenu,
  EmptyState,
  IconButton,
  Meter,
  SearchField,
  Select,
  Tag,
  TextArea,
  TextField,
  useToast,
} from "../components/ui";
import type { BrainsList, ContextPreview, Conversation, StoredMessage } from "../ipc/brains";
import { Method, type PermissionMode, type StepView, type TurnView } from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";
import { cn } from "../lib/cn";
import { modes } from "../lib/session";

/** Attachments are read as text, up to this much in all (the runtime's limit). */
const ATTACH_LIMIT = 200_000;

interface Attachment {
  name: string;
  text: string;
}

export function Chat({ onOpenPermissions }: { onOpenPermissions: () => void }) {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const fail = useCallback((e: unknown) => toast(e instanceof Error ? e.message : String(e)), [toast]);
  const connected = link?.status === "connected";
  const snapshot = link?.status === "connected" ? link.snapshot : null;
  const [threads, setThreads] = useState<Conversation[]>([]);
  const [current, setCurrent] = useState<string | null>(null);
  const [messages, setMessages] = useState<StoredMessage[]>([]);
  const [thread, setThread] = useState<Conversation | null>(null);
  const [query, setQuery] = useState("");
  const [found, setFound] = useState<StoredMessage[] | null>(null);
  const [text, setText] = useState("");
  const [profile, setProfile] = useState("auto");
  const [brains, setBrains] = useState<BrainsList | null>(null);
  const [attachments, setAttachments] = useState<Attachment[]>([]);
  /** The turn this page started, so its live answer shows here. */
  const [ownTurn, setOwnTurn] = useState<{ thread: string; before: string | null } | null>(null);
  const [renaming, setRenaming] = useState<string | null>(null);
  /** The thread's context between turns: what the next message would start with. */
  const [context, setContext] = useState<{ used: number; budget: number } | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const bottom = useRef<HTMLDivElement>(null);

  const loadThreads = useCallback(() => {
    if (!connected) return;
    void request<Conversation[]>(Method.chatThreads, { limit: 200 })
      .then((list) => {
        setThreads(list);
        setCurrent((c) => c ?? list[0]?.id ?? null);
      })
      .catch(() => {});
  }, [connected, request]);
  const loadThread = useCallback(() => {
    if (!connected || !current) return;
    void request<{ thread: Conversation; messages: StoredMessage[] }>(Method.chatThread, { id: current })
      .then((r) => {
        setThread(r.thread);
        setMessages(r.messages);
      })
      .catch(() => {
        setThread(null);
        setMessages([]);
      });
    void request<ContextPreview>(Method.brainsContext, { thread: current })
      .then((p) => {
        const used = p.layers.filter((l) => l.on !== false).reduce((sum, l) => sum + l.tokens, 0);
        setContext(p.brain ? { used, budget: p.budget.chat } : null);
      })
      .catch(() => setContext(null));
  }, [connected, current, request]);
  useEffect(loadThreads, [loadThreads]);
  useEffect(loadThread, [loadThread]);
  useEffect(() => {
    if (!connected) return;
    void request<BrainsList>(Method.brainsList)
      .then(setBrains)
      .catch(() => {});
  }, [connected, request]);
  useRuntimeEvents((event) => {
    if (event.group === "provider" && event.event.type === "threadChanged") {
      loadThreads();
      if (event.event.thread === current) loadThread();
    }
    if (event.group === "turn" && (event.event.type === "completed" || event.event.type === "failed")) loadThread();
  });
  // Follow the conversation as it grows (new messages, the streamed answer).
  const answerLength = snapshot?.turn?.answer?.length ?? 0;
  useEffect(() => {
    if (messages.length > 0 || answerLength > 0) bottom.current?.scrollIntoView?.({ block: "end" });
  }, [messages, answerLength]);

  // The live turn is ours when it started after we sent, and this thread is open.
  const turn: TurnView | null =
    ownTurn && ownTurn.thread === current && snapshot?.turn && snapshot.turn.id !== ownTurn.before
      ? snapshot.turn
      : null;
  const busy = !!turn && snapshot?.session !== "idle";
  const answerInThread = turn?.answer && messages.some((m) => m.turnId === turn.id);

  const send = async () => {
    const body = text.trim();
    if (!body) return;
    let id = current;
    if (!id) {
      try {
        const created = await request<Conversation>(Method.chatNew, { title: body.slice(0, 60) });
        id = created.id;
        setCurrent(id);
      } catch (e) {
        fail(e);
        return;
      }
    }
    const target = id;
    setOwnTurn({ thread: target, before: snapshot?.turn?.id ?? null });
    request(Method.chatSend, {
      thread: target,
      text: body,
      profile: profile === "auto" ? null : profile,
      attachments,
    })
      .then(() => {
        setText("");
        setAttachments([]);
        loadThread();
      })
      .catch(fail);
  };

  const attach = (files: FileList | null) => {
    if (!files) return;
    void Promise.all(
      Array.from(files).map(
        (f) =>
          new Promise<Attachment | null>((resolve) => {
            f.text()
              .then((body) => resolve({ name: f.name, text: body }))
              .catch(() => resolve(null));
          }),
      ),
    ).then((read) => {
      const next = [...attachments, ...read.filter((a): a is Attachment => a !== null)];
      if (next.reduce((n, a) => n + a.text.length, 0) > ATTACH_LIMIT) {
        toast(t("chat.attachTooBig"));
        return;
      }
      setAttachments(next);
    });
  };

  const search = (q: string) => {
    setQuery(q);
    if (!q.trim()) {
      setFound(null);
      return;
    }
    void request<StoredMessage[]>(Method.chatSearch, { query: q })
      .then(setFound)
      .catch(() => setFound([]));
  };

  const chooseMode = (next: PermissionMode) => {
    if (next === "bypass") onOpenPermissions();
    else void request(Method.permissionsSetMode, { mode: next }).catch(fail);
  };

  const day = useMemo(
    () => new Intl.DateTimeFormat(i18n.language, { weekday: "short", hour: "2-digit", minute: "2-digit" }),
    [i18n.language],
  );
  const pinned = threads.filter((c) => c.pinned);
  const recent = threads.filter((c) => !c.pinned);
  const chip = turn?.brain ?? null;
  // The live turn's meter while it runs; otherwise what the next message would start with.
  const contextPercent =
    chip && chip.contextBudget > 0
      ? Math.round((chip.contextUsed / chip.contextBudget) * 100)
      : context && context.budget > 0
        ? Math.round((context.used / context.budget) * 100)
        : null;
  const branch = (id: string, message?: number) =>
    void request<Conversation>(Method.chatBranch, { id, ...(message === undefined ? {} : { message }) })
      .then((c) => {
        setCurrent(c.id);
        loadThreads();
        toast(t("chat.branched"));
      })
      .catch(fail);
  const exportAs = (id: string, json: boolean) =>
    void request<{ file: string }>(Method.chatExport, { id, json, save: true })
      .then((r) => toast(t("chat.exported", { file: r.file })))
      .catch(fail);

  if (!connected) {
    return (
      <EmptyState icon="chat" title={t("chat.notConnected")}>
        {t("brains.notConnected")}
      </EmptyState>
    );
  }

  return (
    <div className="k-chat">
      <aside className="k-chat__threads" aria-label={t("chat.threads")}>
        <Button
          icon="add"
          onClick={() =>
            void request<Conversation>(Method.chatNew, {})
              .then((c) => {
                setCurrent(c.id);
                loadThreads();
              })
              .catch(fail)
          }
        >
          {t("chat.new")}
        </Button>
        <SearchField
          value={query}
          placeholder={t("chat.search")}
          aria-label={t("chat.search")}
          onChange={(e) => search(e.target.value)}
        />
        {found ? (
          <div className="k-chat__list">
            {found.length === 0 && <p className="k-meta">{t("chat.noResults")}</p>}
            {found.map((m) => (
              <button
                type="button"
                key={m.id}
                className="k-chat__thread"
                onClick={() => {
                  setCurrent(m.conversationId);
                  setFound(null);
                  setQuery("");
                }}
              >
                <b>{m.text.slice(0, 80)}</b>
                <small>{day.format(new Date(m.ts))}</small>
              </button>
            ))}
          </div>
        ) : (
          <div className="k-chat__list">
            {pinned.length > 0 && <div className="k-chat__group">{t("chat.pinned")}</div>}
            {pinned.map((c) => (
              <ThreadButton key={c.id} c={c} on={c.id === current} onClick={() => setCurrent(c.id)} format={day} />
            ))}
            {recent.length > 0 && <div className="k-chat__group">{t("chat.recent")}</div>}
            {recent.map((c) => (
              <ThreadButton key={c.id} c={c} on={c.id === current} onClick={() => setCurrent(c.id)} format={day} />
            ))}
            {threads.length === 0 && <p className="k-meta">{t("chat.noThreads")}</p>}
          </div>
        )}
      </aside>

      <section className="k-chat__conv" aria-label={t("chat.conversation")}>
        <header className="k-chat__head">
          <b className="k-chat__title">{thread?.title || t("chat.untitled")}</b>
          {chip && (
            <span title={chip.reason}>
              <Tag tone="accent">{`${chip.profile} · ${chip.name}`}</Tag>
            </span>
          )}
          {contextPercent !== null && (
            <span className="k-chat__meter" title={t("chat.contextUsed")}>
              <Meter value={Math.min(100, contextPercent)} label={t("chat.contextUsed")} />
              {t("chat.contextPercent", { percent: contextPercent })}
            </span>
          )}
          {chip?.cost != null && <span className="k-meta">≈ ${chip.cost.toFixed(3)}</span>}
          {thread && (
            <DropdownMenu
              align="end"
              trigger={<IconButton icon="more" label={t("chat.more")} size="sm" />}
              items={[
                { type: "item", label: t("chat.rename"), icon: "edit", onSelect: () => setRenaming(thread.title) },
                {
                  type: "item",
                  label: thread.pinned ? t("chat.unpin") : t("chat.pin"),
                  icon: "grip",
                  onSelect: () =>
                    void request(Method.chatUpdate, { id: thread.id, pinned: !thread.pinned })
                      .then(loadThreads)
                      .catch(fail),
                },
                {
                  type: "item",
                  label: t("chat.continue"),
                  icon: "mic",
                  onSelect: () =>
                    void request(Method.chatContinue, { id: thread.id })
                      .then(() => toast(t("chat.continued")))
                      .catch(fail),
                },
                { type: "item", label: t("chat.branch"), icon: "branch", onSelect: () => branch(thread.id) },
                {
                  type: "item",
                  label: t("chat.exportMarkdown"),
                  icon: "download",
                  onSelect: () => exportAs(thread.id, false),
                },
                {
                  type: "item",
                  label: t("chat.exportJson"),
                  icon: "download",
                  onSelect: () => exportAs(thread.id, true),
                },
                { type: "separator" },
                {
                  type: "item",
                  label: t("chat.compact"),
                  icon: "compress",
                  onSelect: () =>
                    void request(Method.chatCompact, { id: thread.id })
                      .then(() => {
                        toast(t("chat.compacted"));
                        loadThread();
                      })
                      .catch(fail),
                },
                { type: "separator" },
                {
                  type: "item",
                  label: t("chat.delete"),
                  icon: "delete",
                  danger: true,
                  onSelect: () =>
                    void request(Method.chatDelete, { id: thread.id })
                      .then(() => {
                        setCurrent(null);
                        setThread(null);
                        setMessages([]);
                        loadThreads();
                      })
                      .catch(fail),
                },
              ]}
            />
          )}
        </header>

        <div className="k-chat__msgs" aria-live="polite">
          {thread?.summary && <p className="k-chat__summary">{t("chat.summary", { summary: thread.summary })}</p>}
          {messages.length === 0 && !turn && <p className="k-meta">{t("chat.empty")}</p>}
          {messages.map((m) =>
            m.role === "user" ? (
              <div key={m.id} className="k-chat__you">
                {m.text}
              </div>
            ) : (
              <div key={m.id} className="k-chat__kivo">
                <span className="k-chat__orb" aria-hidden />
                <div>
                  <p className="k-chat__text">{m.text}</p>
                  <div className="k-chat__why">
                    {m.brain && <span>{m.brain}</span>}
                    <button type="button" className="k-link" onClick={() => branch(m.conversationId, m.id)}>
                      {t("chat.branchHere")}
                    </button>
                    {m.turnId && (
                      <button
                        type="button"
                        className="k-link"
                        onClick={() =>
                          void request(Method.chatMisroute, { turnId: m.turnId, note: "" })
                            .then(() => toast(t("chat.misrouteThanks")))
                            .catch(fail)
                        }
                      >
                        {t("chat.misroute")}
                      </button>
                    )}
                  </div>
                </div>
              </div>
            ),
          )}
          {turn && (
            <div className="k-chat__kivo">
              <span className="k-chat__orb is-live" aria-hidden />
              <div style={{ flex: 1, minWidth: 0 }}>
                {turn.steps.map((s) => (
                  <StepLine key={s.id} step={s} />
                ))}
                {!answerInThread && turn.answer && <p className="k-chat__text">{turn.answer}</p>}
                {turn.error && <Alert kind="warning" title={turn.error} />}
                {turn.confirm && (
                  <Alert kind="warning" title={t("chat.needsOk")}>
                    <p>{turn.confirm.action}</p>
                    <p className="k-meta">{turn.confirm.why}</p>
                    <div className="k-chat__confirm">
                      <Button
                        size="sm"
                        variant="primary"
                        onClick={() =>
                          void request(Method.permissionsAnswer, {
                            callId: turn.confirm?.callId,
                            allow: true,
                            always: false,
                          }).catch(fail)
                        }
                      >
                        {t("chat.allowOnce")}
                      </Button>
                      {turn.confirm.allowAlways && (
                        <Button
                          size="sm"
                          onClick={() =>
                            void request(Method.permissionsAnswer, {
                              callId: turn.confirm?.callId,
                              allow: true,
                              always: true,
                            }).catch(fail)
                          }
                        >
                          {t("chat.allowAlways")}
                        </Button>
                      )}
                      <Button
                        size="sm"
                        variant="destructive"
                        onClick={() =>
                          void request(Method.permissionsAnswer, {
                            callId: turn.confirm?.callId,
                            allow: false,
                            always: false,
                          }).catch(fail)
                        }
                      >
                        {t("chat.deny")}
                      </Button>
                    </div>
                  </Alert>
                )}
                {chip && <div className="k-chat__why">{chip.reason}</div>}
              </div>
            </div>
          )}
          <div ref={bottom} />
        </div>

        {attachments.length > 0 && (
          <div className="k-chat__attachments">
            {attachments.map((a) => (
              <Tag key={a.name}>
                {a.name}
                <button
                  type="button"
                  className="k-link"
                  aria-label={t("chat.removeAttachment", { name: a.name })}
                  onClick={() => setAttachments(attachments.filter((x) => x !== a))}
                >
                  ×
                </button>
              </Tag>
            ))}
          </div>
        )}
        <div className="k-chat__composer">
          <input
            ref={fileInput}
            type="file"
            hidden
            multiple
            accept=".txt,.md,.json,.csv,.log,.rs,.ts,.tsx,.js,.py,.toml,.yaml,.yml,.xml,.html,.css"
            onChange={(e) => attach(e.target.files)}
          />
          <IconButton icon="add" label={t("chat.attach")} onClick={() => fileInput.current?.click()} />
          <TextArea
            className="k-chat__input"
            rows={1}
            value={text}
            placeholder={t("chat.placeholder")}
            aria-label={t("chat.placeholder")}
            onChange={(e) => setText(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void send();
              }
            }}
          />
          <Select
            label={t("chat.brain")}
            value={profile}
            onChange={setProfile}
            items={[
              { value: "auto", label: t("chat.automatic") },
              ...(brains?.profiles ?? []).map((p) => ({ value: p.id, label: p.name })),
            ]}
          />
          {snapshot && (
            <Select
              label={t("home.mode")}
              icon="permissions"
              items={modes()}
              value={snapshot.mode}
              onChange={chooseMode}
            />
          )}
          <IconButton icon="mic" label={t("chat.talk")} onClick={() => void request(Method.sessionTalk).catch(fail)} />
          {busy ? (
            <Button variant="stop" icon="stop" onClick={() => void request(Method.sessionCancel).catch(fail)}>
              {t("chat.stop")}
            </Button>
          ) : (
            <Button variant="primary" icon="send" disabled={!text.trim()} onClick={() => void send()}>
              {t("chat.send")}
            </Button>
          )}
        </div>
      </section>

      {renaming !== null && thread && (
        <Dialog
          open
          onOpenChange={(o) => !o && setRenaming(null)}
          title={t("chat.rename")}
          footer={
            <Button
              variant="primary"
              onClick={() =>
                void request(Method.chatUpdate, { id: thread.id, title: renaming })
                  .then(() => {
                    setRenaming(null);
                    loadThreads();
                    loadThread();
                  })
                  .catch(fail)
              }
            >
              {t("brains.save")}
            </Button>
          }
        >
          <TextField value={renaming} onChange={(e) => setRenaming(e.target.value)} aria-label={t("chat.rename")} />
        </Dialog>
      )}
    </div>
  );
}

function ThreadButton({
  c,
  on,
  onClick,
  format,
}: {
  c: Conversation;
  on: boolean;
  onClick: () => void;
  format: Intl.DateTimeFormat;
}) {
  const { t } = useTranslation();
  return (
    <button
      type="button"
      className={cn("k-chat__thread", on && "is-on")}
      aria-current={on ? "true" : undefined}
      onClick={onClick}
    >
      <b>{c.title || t("chat.untitled")}</b>
      <small>
        {format.format(new Date(c.updatedAt))}
        {c.kind === "voice" ? ` · ${t("chat.voice")}` : ""}
        {c.brain ? ` · ${c.brain}` : ""}
      </small>
    </button>
  );
}

function StepLine({ step }: { step: StepView }) {
  return (
    <div className={cn("k-chat__step", `is-${step.status}`)}>
      <span className="k-chat__step-dot" aria-hidden />
      {step.title}
      {step.detail && <span className="k-meta"> · {step.detail}</span>}
    </div>
  );
}
