/**
 * Memory (UX-29, MEM-06, CONVERSATION §6): what KIVO knows about the user and their work — plain
 * Markdown notes on this PC, organized by folders and tags, that Obsidian can open.
 *
 * - How memory is captured (Suggest, workspace notes, note detail) and what KIVO suggested
 *   remembering, to keep, edit or dismiss (CONV-19/20).
 * - The vault by folders and tags, with search; a note's preview with its links, the notes that
 *   link to it and, for a fact, the facts it replaced (CONV-17/18). Notes are edited as Markdown.
 * - Keeping it tidy, cloud sharing of sensitive notes, agents' access, export and Forget
 *   everything.
 *
 * The runtime owns the files; the page reloads when it says memory changed.
 */
import { Fragment, useCallback, useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import { MemoryGraph } from "../components/memory/MemoryGraph";
import {
  Button,
  Dialog,
  EmptyState,
  Group,
  Meta,
  Note,
  Row,
  SearchField,
  Section,
  Segmented,
  Switch,
  Tag,
  TextArea,
  TextField,
  useToast,
} from "../components/ui";
import { Icon, type IconName } from "../icons";
import {
  Method,
  type MemoryNoteDetail,
  type MemoryNoteView,
  type MemoryOverview,
  type MemorySuggestionView,
} from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";
import { ago } from "../lib/ago";
import { bool, oneOf, setting, useSettings } from "../lib/settings";

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

const EMPTY: MemoryOverview = { root: "", notes: [], tags: [], folders: [], suggestions: [], links: [] };

/** The overview, reloaded whenever the runtime says memory changed. */
function useOverview(): [MemoryOverview, () => void] {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [overview, setOverview] = useState<MemoryOverview>(EMPTY);
  const load = useCallback(() => {
    if (!connected) return;
    void request<MemoryOverview>(Method.memoryOverview)
      .then(setOverview)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  useRuntimeEvents((event) => {
    if (event.group === "system" && event.event.type === "discoveryChanged" && event.event.section === "memory") {
      load();
    }
  });
  return [overview, load];
}

/** Where the tree puts a folder: its icon and label. */
function folderLook(path: string, t: (k: string) => string): { icon: IconName; label: string } {
  const top = path.split("/")[0] ?? path;
  const last = path.split("/").pop() ?? path;
  if (path === "people") return { icon: "users", label: t("memory.folders.people") };
  if (path === "workspaces") return { icon: "folder", label: t("memory.folders.workspaces") };
  if (path === "topics") return { icon: "list", label: t("memory.folders.topics") };
  if (path === "notes") return { icon: "idea", label: t("memory.folders.notes") };
  if (path === "archive") return { icon: "clock", label: t("memory.folders.archive") };
  if (top === "people") return { icon: "user", label: last };
  if (last === "log") return { icon: "clock", label: t("memory.folders.log") };
  return { icon: "folder", label: last };
}

type Selection = { kind: "folder"; path: string } | { kind: "note"; path: string } | { kind: "about" };

export function Memory({ onOpenAgents }: { onOpenAgents?: () => void }) {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const [overview, reload] = useOverview();
  const [settings, save] = useSettings((e) => toast(message(e)));
  const [query, setQuery] = useState("");
  const [tag, setTag] = useState<string | null>(null);
  const [view, setView] = useState<"notes" | "graph">("notes");
  const [selected, setSelected] = useState<Selection>({ kind: "about" });
  const [remember, setRemember] = useState(false);
  const [forget, setForget] = useState(false);

  if (link?.status !== "connected") {
    return (
      <>
        <PageHeader title={t("nav.memory")} subtitle={t("memory.subtitle")} />
        <Note>{t("voice.notConnected")}</Note>
      </>
    );
  }

  const memorySetting = (key: string) => setting(settings, "memory", key);
  const saveMemory = (patch: Record<string, unknown>) => save({ memory: patch });
  const open = (where: "folder" | "obsidian", path?: string) =>
    void request(Method.memoryOpen, path ? { in: where, path } : { in: where }).catch((e: unknown) =>
      toast(message(e)),
    );

  const filtering = query.trim() !== "" || tag !== null;
  const aboutMe = overview.notes.find((n) => n.path === "about-me.md");

  return (
    <>
      <PageHeader
        title={t("nav.memory")}
        subtitle={t("memory.subtitle")}
        actions={
          <>
            <div style={{ width: 200 }}>
              <SearchField
                aria-label={t("memory.search")}
                placeholder={t("memory.search")}
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
            </div>
            <Button icon="folder" onClick={() => open("folder")}>
              {t("memory.openFolder")}
            </Button>
            <Button icon="external" onClick={() => open("obsidian")}>
              {t("memory.openObsidian")}
            </Button>
          </>
        }
      />

      <Group>
        <Row
          icon="idea"
          title={t("memory.suggest")}
          subtitle={t("memory.suggestDetail")}
          end={
            <Switch
              label={t("memory.suggest")}
              checked={oneOf(memorySetting("capture"), ["suggest", "only-when-asked"]) !== "only-when-asked"}
              onChange={(on) => saveMemory({ capture: on ? "suggest" : "only-when-asked" })}
            />
          }
        />
        <Row
          icon="folder"
          title={t("memory.workspaceNotes")}
          subtitle={t("memory.workspaceNotesDetail")}
          end={
            <Switch
              label={t("memory.workspaceNotes")}
              checked={bool(memorySetting("workspace-notes"), true)}
              onChange={(on) => saveMemory({ "workspace-notes": on })}
            />
          }
        />
        <Row
          icon="help"
          title={t("memory.askNew")}
          subtitle={t("memory.askNewDetail")}
          end={
            <Switch
              label={t("memory.askNew")}
              checked={bool(memorySetting("ask-new-notes"), true)}
              onChange={(on) => saveMemory({ "ask-new-notes": on })}
            />
          }
        />
        <Row
          icon="list"
          title={t("memory.detail")}
          end={
            <Segmented
              label={t("memory.detail")}
              value={oneOf(memorySetting("detail"), ["brief", "standard", "detailed"]) ?? "standard"}
              onChange={(v) => saveMemory({ detail: v })}
              options={[
                { value: "brief", label: t("memory.details.brief") },
                { value: "standard", label: t("memory.details.standard") },
                { value: "detailed", label: t("memory.details.detailed") },
              ]}
            />
          }
        />
      </Group>

      <Suggestions suggestions={overview.suggestions} onChanged={reload} />

      <Section
        title={t("memory.tags")}
        aside={
          <>
            <Segmented
              label={t("memory.view")}
              value={view}
              onChange={setView}
              options={[
                { value: "notes", label: t("memory.viewNotes") },
                { value: "graph", label: t("memory.viewGraph") },
              ]}
            />
            <Button size="sm" variant="plain" icon="add" onClick={() => setRemember(true)}>
              {t("memory.remember")}
            </Button>
          </>
        }
      />
      <div className="k-chips" role="group" aria-label={t("memory.tags")}>
        <button type="button" aria-pressed={tag === null} onClick={() => setTag(null)}>
          {t("memory.allTags")}
        </button>
        {overview.tags.map((x) => (
          <button
            key={x.tag}
            type="button"
            aria-pressed={tag === x.tag}
            onClick={() => setTag(tag === x.tag ? null : x.tag)}
          >
            #{x.tag}
            <span className="k-chips__n">{x.count}</span>
          </button>
        ))}
      </div>

      {overview.notes.length > 0 && view === "graph" ? (
        <MemoryGraph
          notes={overview.notes}
          links={overview.links}
          onOpen={(path) => {
            setView("notes");
            setSelected({ kind: "note", path });
            setQuery("");
            setTag(null);
          }}
          onTag={(x) => {
            setView("notes");
            setTag(x);
          }}
        />
      ) : overview.notes.length === 0 ? (
        <div style={{ marginTop: 12 }}>
          <EmptyState icon="memory" title={t("memory.empty")}>
            {t("memory.emptyDetail")}
          </EmptyState>
        </div>
      ) : (
        <div className="k-vault">
          <Tree
            overview={overview}
            selected={selected}
            onSelect={(s) => {
              setSelected(s);
              setQuery("");
              setTag(null);
            }}
          />
          <div className="k-vault__main">
            {filtering ? (
              <NoteList
                notes={filterNotes(overview.notes, query, tag)}
                onOpen={(path) => {
                  setSelected({ kind: "note", path });
                  setQuery("");
                  setTag(null);
                }}
              />
            ) : selected.kind === "folder" ? (
              <NoteList
                notes={overview.notes.filter((n) => n.folder === selected.path)}
                onOpen={(path) => setSelected({ kind: "note", path })}
              />
            ) : selected.kind === "note" ? (
              <NotePreview
                key={selected.path}
                path={selected.path}
                notes={overview.notes}
                onOpen={(path) => setSelected({ kind: "note", path })}
                onGone={() => setSelected({ kind: "about" })}
                onOpenFile={(path) => open("obsidian", path)}
              />
            ) : aboutMe ? (
              <NotePreview
                key="about-me.md"
                path="about-me.md"
                notes={overview.notes}
                onOpen={(path) => setSelected({ kind: "note", path })}
                onGone={() => setSelected({ kind: "about" })}
                onOpenFile={(path) => open("obsidian", path)}
              />
            ) : (
              <EmptyState icon="user" title={t("memory.noAbout")}>
                {t("memory.noAboutDetail")}
              </EmptyState>
            )}
          </div>
        </div>
      )}

      <Section title={t("memory.tidyTitle")} />
      <Group>
        <Row
          icon="merge"
          title={t("memory.merge")}
          end={
            <Switch
              label={t("memory.merge")}
              checked={bool(memorySetting("merge-duplicates"), true)}
              onChange={(on) => saveMemory({ "merge-duplicates": on })}
            />
          }
        />
        <Row
          icon="compress"
          title={t("memory.condense")}
          subtitle={t("memory.condenseDetail")}
          end={
            <Switch
              label={t("memory.condense")}
              checked={bool(memorySetting("condense-logs"), true)}
              onChange={(on) => saveMemory({ "condense-logs": on })}
            />
          }
        />
        <Row
          icon="privacy"
          title={t("memory.noCloud")}
          subtitle={t("memory.noCloudDetail")}
          end={
            <Switch
              label={t("memory.noCloud")}
              checked={!bool(memorySetting("sensitive-to-cloud"), false)}
              onChange={(on) => saveMemory({ "sensitive-to-cloud": !on })}
            />
          }
        />
        <Row icon="agent" title={t("memory.agents")} subtitle={t("memory.agentsDetail")} onClick={onOpenAgents} />
        <Row
          icon="refresh"
          title={t("memory.tidyNow")}
          subtitle={t("memory.tidyNowDetail")}
          end={
            <Button
              size="sm"
              onClick={() =>
                void request<{ merged: number; superseded: number; condensedLogs: number }>(Method.memoryTidy)
                  .then((r) =>
                    toast(t("memory.tidied", { merged: r.merged, superseded: r.superseded, logs: r.condensedLogs })),
                  )
                  .catch((e: unknown) => toast(message(e)))
              }
            >
              {t("memory.tidy")}
            </Button>
          }
        />
        <Row
          icon="download"
          title={t("memory.export")}
          subtitle={t("memory.exportDetail")}
          end={
            <Button
              size="sm"
              onClick={() =>
                void request<{ file: string }>(Method.memoryExport)
                  .then((r) => toast(t("memory.exported", { file: r.file })))
                  .catch((e: unknown) => toast(message(e)))
              }
            >
              {t("memory.exportButton")}
            </Button>
          }
        />
        <Row
          icon="delete"
          title={t("memory.forget")}
          subtitle={t("memory.forgetDetail")}
          end={
            <Button size="sm" variant="destructive" onClick={() => setForget(true)}>
              {t("memory.forgetButton")}
            </Button>
          }
        />
      </Group>
      <Note>{t("memory.note", { folder: overview.root })}</Note>

      <RememberDialog open={remember} onOpenChange={setRemember} tags={overview.tags.map((x) => x.tag)} />
      <Dialog
        open={forget}
        onOpenChange={setForget}
        title={t("memory.forgetTitle")}
        description={t("memory.forgetConfirm", { count: overview.notes.length })}
        footer={
          <>
            <Button variant="plain" onClick={() => setForget(false)}>
              {t("ui.cancel")}
            </Button>
            <Button
              variant="destructive"
              onClick={() => {
                setForget(false);
                void request<{ forgotten: number }>(Method.memoryForget)
                  .then((r) => {
                    toast(t("memory.forgotten", { count: r.forgotten }));
                    setSelected({ kind: "about" });
                  })
                  .catch((e: unknown) => toast(message(e)));
              }}
            >
              {t("memory.forgetButton")}
            </Button>
          </>
        }
      />
    </>
  );
}

function filterNotes(notes: MemoryNoteView[], query: string, tag: string | null): MemoryNoteView[] {
  const q = query.trim().toLowerCase();
  return notes.filter(
    (n) =>
      (tag === null || n.tags.includes(tag) || (tag === "sensitive" && isSensitive(n))) &&
      (q === "" || `${n.title} ${n.excerpt} ${n.path} ${n.tags.join(" ")}`.toLowerCase().includes(q)),
  );
}

function isSensitive(n: MemoryNoteView): boolean {
  return ["sensitive", "credential", "highly_sensitive"].includes(n.sensitivity);
}

/* ───────────────────────────── Suggestions ───────────────────────────── */

function Suggestions({ suggestions, onChanged }: { suggestions: MemorySuggestionView[]; onChanged: () => void }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [editing, setEditing] = useState<MemorySuggestionView | null>(null);
  const [text, setText] = useState("");
  if (suggestions.length === 0) return null;
  const answer = (id: number, accept: boolean, edited?: string) =>
    void request(Method.memorySuggestion, edited === undefined ? { id, accept } : { id, accept, text: edited })
      .then(() => {
        if (accept) toast(t("memory.kept"));
        onChanged();
      })
      .catch((e: unknown) => toast(message(e)));
  return (
    <>
      <Section title={t("memory.suggestions")} aside={<Meta>{suggestions.length}</Meta>} />
      <Group>
        {suggestions.map((s) => (
          <Row
            key={s.id}
            icon="idea"
            title={s.text}
            subtitle={[s.reason, s.workspace].filter(Boolean).join(" · ")}
            end={
              <>
                <Button size="sm" variant="plain" onClick={() => answer(s.id, false)}>
                  {t("memory.dismiss")}
                </Button>
                <Button
                  size="sm"
                  variant="plain"
                  onClick={() => {
                    setEditing(s);
                    setText(s.text);
                  }}
                >
                  {t("memory.edit")}
                </Button>
                <Button size="sm" variant="primary" onClick={() => answer(s.id, true)}>
                  {t("memory.keep")}
                </Button>
              </>
            }
          />
        ))}
      </Group>
      <Dialog
        open={editing !== null}
        onOpenChange={(o) => !o && setEditing(null)}
        title={t("memory.editSuggestion")}
        footer={
          <>
            <Button variant="plain" onClick={() => setEditing(null)}>
              {t("ui.cancel")}
            </Button>
            <Button
              variant="primary"
              disabled={!text.trim()}
              onClick={() => {
                if (editing) answer(editing.id, true, text);
                setEditing(null);
              }}
            >
              {t("memory.keep")}
            </Button>
          </>
        }
      >
        <TextArea
          aria-label={t("memory.editSuggestion")}
          rows={3}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />
      </Dialog>
    </>
  );
}

/* ───────────────────────────── The tree ───────────────────────────── */

function Tree({
  overview,
  selected,
  onSelect,
}: {
  overview: MemoryOverview;
  selected: Selection;
  onSelect: (s: Selection) => void;
}) {
  const { t } = useTranslation();
  const counts = new Map(overview.folders.map((f) => [f.path, f.count]));
  // Top folders, then their sub-folders one level down (people/…, workspaces/<name>).
  const top = overview.folders.filter((f) => !f.path.includes("/"));
  const order = ["people", "workspaces", "topics", "notes", "archive"];
  const sorted = top.toSorted((a, b) => {
    const ia = order.indexOf(a.path);
    const ib = order.indexOf(b.path);
    return (ia < 0 ? 99 : ia) - (ib < 0 ? 99 : ib) || a.path.localeCompare(b.path);
  });
  const people = overview.notes.filter((n) => n.folder === "people");
  const isOn = (s: Selection) =>
    s.kind === selected.kind && (s.kind === "about" || ("path" in s && "path" in selected && s.path === selected.path));
  const link = (s: Selection, icon: IconName, label: ReactNode, count?: number) => (
    <button type="button" className={isOn(s) ? "on" : undefined} aria-current={isOn(s)} onClick={() => onSelect(s)}>
      <Icon name={icon} size={15} />
      <span className="k-tree__label">{label}</span>
      {count !== undefined && <span className="k-tree__n">{count}</span>}
    </button>
  );
  return (
    <nav className="k-tree" aria-label={t("memory.folders.label")}>
      {link({ kind: "about" }, "user", t("memory.folders.about"))}
      {sorted.map((f) => {
        const look = folderLook(f.path, t);
        const subfolders = overview.folders.filter(
          (s) => s.path.startsWith(`${f.path}/`) && s.path.split("/").length === 2,
        );
        return (
          <Fragment key={f.path}>
            {link({ kind: "folder", path: f.path }, look.icon, look.label, counts.get(f.path))}
            {f.path === "people" && people.length > 0 && (
              <div className="k-tree__in">
                {people.map((p) => (
                  <Fragment key={p.path}>{link({ kind: "note", path: p.path }, "user", p.title)}</Fragment>
                ))}
              </div>
            )}
            {f.path !== "people" && f.path !== "archive" && subfolders.length > 0 && (
              <div className="k-tree__in">
                {subfolders.map((s) => {
                  const overviewNote = overview.notes.find((n) => n.path === `${s.path}/overview.md`);
                  const l = folderLook(s.path, t);
                  return (
                    <Fragment key={s.path}>
                      {link(
                        overviewNote ? { kind: "note", path: overviewNote.path } : { kind: "folder", path: s.path },
                        l.icon,
                        overviewNote?.title ?? l.label,
                        counts.get(s.path),
                      )}
                    </Fragment>
                  );
                })}
              </div>
            )}
          </Fragment>
        );
      })}
    </nav>
  );
}

function NoteList({ notes, onOpen }: { notes: MemoryNoteView[]; onOpen: (path: string) => void }) {
  const { t, i18n } = useTranslation();
  if (notes.length === 0) {
    return <EmptyState icon="search" title={t("memory.nothingFound")} />;
  }
  return (
    <Group>
      {notes.map((n) => (
        <Row
          key={n.path}
          icon={n.kind === "person" ? "user" : n.kind === "fact" ? "idea" : "file"}
          title={n.title}
          subtitle={n.excerpt || n.path}
          onClick={() => onOpen(n.path)}
          end={
            <>
              {n.validUntil !== null && <Tag>{t("memory.replaced")}</Tag>}
              {isSensitive(n) && <Tag tone="warning">{t("memory.sensitive")}</Tag>}
              <Meta>{ago(n.updatedAt, i18n.language)}</Meta>
            </>
          }
        />
      ))}
    </Group>
  );
}

/* ───────────────────────────── A note ───────────────────────────── */

/** A note's body as React: headings, bullets, paragraphs, `code` and clickable [[links]]. */
function MarkdownBody({ body, onLink }: { body: string; onLink: (target: string) => void }) {
  const inline = (text: string, key: string): ReactNode[] =>
    text.split(/(\[\[[^\]]+\]\]|`[^`]+`)/g).map((part, i) => {
      const k = `${key}-${i}`;
      if (part.startsWith("[[") && part.endsWith("]]")) {
        const inner = part.slice(2, -2);
        const [target = inner, alias] = inner.split("|");
        return (
          <button key={k} type="button" className="k-wikilink" onClick={() => onLink(target.split("#")[0] ?? target)}>
            {alias ?? target}
          </button>
        );
      }
      if (part.startsWith("`") && part.endsWith("`") && part.length > 1)
        return <code key={k}>{part.slice(1, -1)}</code>;
      return <Fragment key={k}>{part}</Fragment>;
    });
  const blocks: ReactNode[] = [];
  let bullets: string[] = [];
  const flush = (key: string) => {
    if (bullets.length > 0) {
      const items = bullets;
      blocks.push(
        <ul key={key}>
          {items.map((b, i) => (
            <li key={i}>{inline(b, `${key}-${i}`)}</li>
          ))}
        </ul>,
      );
      bullets = [];
    }
  };
  body.split("\n").forEach((line, i) => {
    const trimmed = line.trim();
    const bullet = /^[-*]\s+(.*)$/.exec(trimmed);
    if (bullet) {
      bullets.push(bullet[1] ?? "");
      return;
    }
    flush(`ul${i}`);
    if (trimmed === "") return;
    const heading = /^(#{1,3})\s+(.*)$/.exec(trimmed);
    if (heading) {
      const level = heading[1]?.length ?? 1;
      const content = inline(heading[2] ?? "", `h${i}`);
      blocks.push(level === 1 ? <h4 key={i}>{content}</h4> : <h5 key={i}>{content}</h5>);
    } else {
      blocks.push(<p key={i}>{inline(trimmed, `p${i}`)}</p>);
    }
  });
  flush("ul-end");
  return <div className="k-note-body">{blocks}</div>;
}

function bodyOf(markdown: string): string {
  const m = /^---\r?\n[\s\S]*?\r?\n---\r?\n?/.exec(markdown);
  return m ? markdown.slice(m[0].length) : markdown;
}

function NotePreview({
  path,
  notes,
  onOpen,
  onGone,
  onOpenFile,
}: {
  path: string;
  notes: MemoryNoteView[];
  onOpen: (path: string) => void;
  onGone: () => void;
  onOpenFile: (path: string) => void;
}) {
  const { t, i18n } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [detail, setDetail] = useState<MemoryNoteDetail | null>(null);
  const [editing, setEditing] = useState<string | null>(null);
  const [show, setShow] = useState<"links" | "history" | null>(null);
  const version = notes.find((n) => n.path === path)?.updatedAt;
  const load = useCallback(() => {
    void request<MemoryNoteDetail>(Method.memoryNote, { path })
      .then(setDetail)
      .catch(() => onGone());
  }, [path, request, onGone]);
  useEffect(load, [load, version]);
  const byTarget = useMemo(() => {
    // A link target is a title, a path without `.md`, or a file name.
    return (target: string) => {
      const want = target.toLowerCase();
      return notes.find(
        (n) =>
          n.title.toLowerCase() === want ||
          n.path.toLowerCase() === `${want}.md` ||
          (n.path.split("/").pop() ?? "").toLowerCase() === `${want}.md`,
      );
    };
  }, [notes]);
  if (!detail) return <div className="k-note-prev" aria-busy="true" />;
  const n = detail.note;
  const title = (p: string) => notes.find((x) => x.path === p)?.title ?? p;
  return (
    <article className="k-note-prev" aria-label={n.title}>
      <div className="k-note-prev__fm">
        {n.path}
        {n.tags.length > 0 && ` · ${t("memory.tagsLine", { tags: n.tags.join(", ") })}`}
        {` · ${t("memory.updated", { when: ago(n.updatedAt, i18n.language) })}`}
      </div>
      {n.validUntil !== null && (
        <Note>{t("memory.supersededNote", { date: new Date(n.validUntil).toLocaleDateString(i18n.language) })}</Note>
      )}
      <MarkdownBody
        body={bodyOf(detail.markdown)}
        onLink={(target) => {
          const found = byTarget(target);
          if (found) onOpen(found.path);
          else toast(t("memory.noSuchNote", { name: target }));
        }}
      />
      <div className="k-note-prev__meta">
        {isSensitive(n) && <Tag tone="warning">{t("memory.sensitive")}</Tag>}
        {n.useCount > 0 && (
          <Meta>
            {t("memory.used", { count: n.useCount })}
            {n.lastUsedAt !== null && ` · ${t("memory.lastUsed", { when: ago(n.lastUsedAt, i18n.language) })}`}
          </Meta>
        )}
      </div>
      <div className="k-note-prev__actions">
        <Button size="sm" icon="edit" onClick={() => setEditing(detail.markdown)}>
          {t("memory.edit")}
        </Button>
        <Button
          size="sm"
          variant="plain"
          aria-pressed={show === "links"}
          onClick={() => setShow(show === "links" ? null : "links")}
        >
          {t("memory.linked", { count: detail.backlinks.length + detail.links.length })}
        </Button>
        {detail.history.length > 0 && (
          <Button
            size="sm"
            variant="plain"
            aria-pressed={show === "history"}
            onClick={() => setShow(show === "history" ? null : "history")}
          >
            {t("memory.history")}
          </Button>
        )}
        <Button size="sm" variant="plain" icon="external" onClick={() => onOpenFile(path)}>
          {t("memory.openObsidian")}
        </Button>
        {!["about-me.md"].includes(path) && !path.endsWith("/instructions.md") && (
          <Button
            size="sm"
            variant="plain"
            icon="delete"
            onClick={() =>
              void request(Method.memoryDelete, { path })
                .then(() => {
                  toast(t("memory.deleted"));
                  onGone();
                })
                .catch((e: unknown) => toast(message(e)))
            }
          >
            {t("memory.delete")}
          </Button>
        )}
      </div>
      {isSensitive(n) && (
        <Row
          icon="cloud"
          title={t("memory.shareCloud")}
          subtitle={t("memory.shareCloudDetail")}
          end={
            <Switch
              label={t("memory.shareCloud")}
              checked={n.shareCloud}
              onChange={(on) =>
                void request(Method.memoryMeta, { path, shareCloud: on }).catch((e: unknown) => toast(message(e)))
              }
            />
          }
        />
      )}
      {show === "links" && (
        <Group>
          {detail.links.map((l) => {
            const found = byTarget(l);
            return (
              <Row
                key={`to-${l}`}
                icon="link"
                title={l}
                subtitle={found ? found.path : t("memory.noNoteYet")}
                onClick={found ? () => onOpen(found.path) : undefined}
              />
            );
          })}
          {detail.backlinks.map((b) => (
            <Row
              key={`from-${b}`}
              icon="undo"
              title={title(b)}
              subtitle={t("memory.linksHere")}
              onClick={() => onOpen(b)}
            />
          ))}
          {detail.links.length + detail.backlinks.length === 0 && <Row title={t("memory.noLinks")} />}
        </Group>
      )}
      {show === "history" && (
        <Group>
          {detail.history.map((h) => (
            <Row
              key={h.path}
              icon="clock"
              title={h.excerpt || h.title}
              subtitle={
                h.validUntil === null
                  ? t("memory.current")
                  : t("memory.until", { date: new Date(h.validUntil).toLocaleDateString(i18n.language) })
              }
              onClick={() => onOpen(h.path)}
            />
          ))}
        </Group>
      )}
      <Dialog
        open={editing !== null}
        onOpenChange={(o) => !o && setEditing(null)}
        title={t("memory.editTitle", { name: n.title })}
        description={t("memory.editDetail")}
        footer={
          <>
            <Button variant="plain" onClick={() => setEditing(null)}>
              {t("ui.cancel")}
            </Button>
            <Button
              variant="primary"
              onClick={() => {
                const markdown = editing ?? "";
                setEditing(null);
                void request(Method.memorySave, { path, markdown })
                  .then(() => {
                    toast(t("memory.saved"));
                    load();
                  })
                  .catch((e: unknown) => toast(message(e)));
              }}
            >
              {t("ui.save")}
            </Button>
          </>
        }
      >
        <TextArea
          className="k-mono"
          aria-label={t("memory.editTitle", { name: n.title })}
          rows={14}
          value={editing ?? ""}
          onChange={(e) => setEditing(e.target.value)}
        />
      </Dialog>
    </article>
  );
}

/* ───────────────────────────── Remember ───────────────────────────── */

function RememberDialog({
  open,
  onOpenChange,
  tags,
}: {
  open: boolean;
  onOpenChange: (o: boolean) => void;
  tags: string[];
}) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [text, setText] = useState("");
  const [tagText, setTagText] = useState("");
  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("memory.rememberTitle")}
      description={t("memory.rememberDetail")}
      footer={
        <>
          <Button variant="plain" onClick={() => onOpenChange(false)}>
            {t("ui.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={!text.trim()}
            onClick={() => {
              const list = tagText
                .split(/[,\s]+/)
                .map((x) => x.replace(/^#/, "").trim())
                .filter(Boolean);
              void request<{ already: boolean }>(Method.memoryRemember, { text, tags: list })
                .then((r) => {
                  toast(r.already ? t("memory.alreadyKnown") : t("memory.kept"));
                  setText("");
                  setTagText("");
                  onOpenChange(false);
                })
                .catch((e: unknown) => toast(message(e)));
            }}
          >
            {t("memory.remember")}
          </Button>
        </>
      }
    >
      <TextArea
        aria-label={t("memory.rememberWhat")}
        placeholder={t("memory.rememberPlaceholder")}
        rows={3}
        value={text}
        onChange={(e) => setText(e.target.value)}
      />
      <TextField
        aria-label={t("memory.tagsField")}
        placeholder={tags.slice(0, 3).join(", ")}
        value={tagText}
        onChange={(e) => setTagText(e.target.value)}
      />
    </Dialog>
  );
}
