import { useCallback, useEffect, useMemo, useState } from "react";
import { MODES } from "./lib/modes";
import { useTranslation } from "react-i18next";
import { RuntimeStatus } from "./components/layout/RuntimeStatus";
import { AppWindow, NAV, Sidebar, type PageId } from "./components/layout/Shell";
import {
  CommandPalette,
  ToastProvider,
  TooltipProvider,
  useCommandPaletteHotkey,
  useToast,
  type Command,
} from "./components/ui";
import { Method, type RoutineView } from "./ipc/generated";
import { RuntimeProvider, useRuntime, useRuntimeEvents } from "./ipc/runtime";
import { ThemeProvider } from "./lib/theme";
import { Gallery } from "./pages/Gallery";
import { Activity } from "./pages/Activity";
import { BRAINS_TABS, Brains } from "./pages/Brains";
import { Chat } from "./pages/Chat";
import { Voice } from "./pages/Voice";
import { Home } from "./pages/Home";
import { Onboarding } from "./pages/Onboarding";
import { WhatsNewDialog } from "./components/updates/Updates";
import { PERMISSIONS_TABS, Permissions } from "./pages/Permissions";
import { SETTINGS_TABS, Settings } from "./pages/Settings";
import { SETTINGS_INDEX } from "./pages/settings/index";
import { Agents } from "./pages/Agents";
import { EXTENSION_TABS, Extensions } from "./pages/Extensions";
import { Memory } from "./pages/Memory";
import { Usage } from "./pages/Usage";
import { ThemeSync } from "./lib/ThemeSync";
import { Routines } from "./pages/Routines";
import { Tasks } from "./pages/Tasks";

const PAGES: ReadonlyArray<PageId> = [
  "home",
  "chat",
  "tasks",
  "activity",
  "routines",
  "brains",
  "agents",
  "voice",
  "extensions",
  "permissions",
  "memory",
  "usage",
  "settings",
  "gallery",
];

function isPage(page: string): page is PageId {
  return PAGES.some((p) => p === page);
}

export function App() {
  const [page, setPage] = useState<PageId>("home");
  // What the page was opened for ("tasks/<id>" opens that task, "chat/new" a new conversation).
  const [focus, setFocus] = useState<{ arg: string; at: number } | null>(null);
  // The runtime asks for pages by name (tray "Settings", `--page`, a toast's "Open", the jump
  // list); unknown names open Home.
  const navigate = useCallback((to: string) => {
    const [name = "", arg] = to.split("/", 2);
    setPage(isPage(name) ? name : "home");
    setFocus(arg ? { arg, at: Date.now() } : null);
  }, []);
  return (
    <ThemeProvider>
      <TooltipProvider>
        <ToastProvider>
          <RuntimeProvider onNavigate={navigate}>
            <ThemeSync />
            <Shell page={page} setPage={setPage} navigate={navigate} focus={focus} />
          </RuntimeProvider>
        </ToastProvider>
      </TooltipProvider>
    </ThemeProvider>
  );
}

/** Setup runs until it is finished or skipped (UX-33); `null` while the settings are loading. */
function useOnboarded(): [boolean | null, () => void] {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [onboarded, setOnboarded] = useState<boolean | null>(null);
  useEffect(() => {
    if (!connected) return;
    void request<{ general: Record<string, unknown> }>(Method.settingsGet)
      .then((s) => setOnboarded(s.general["onboarded"] === true))
      .catch(() => setOnboarded(true));
  }, [connected, request]);
  return [onboarded, () => setOnboarded(true)];
}

function Shell({
  page,
  setPage,
  navigate,
  focus,
}: {
  page: PageId;
  setPage: (page: PageId) => void;
  /** A page with what it was opened for ("settings/sounds", "chat/new"). */
  navigate: (to: string) => void;
  focus: { arg: string; at: number } | null;
}) {
  const [palette, setPalette] = useState(false);
  // Routines the palette can run (UX-47), read each time it opens.
  const [routines, setRoutines] = useState<RoutineView[]>([]);
  useCommandPaletteHotkey(setPalette);
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const session = link?.status === "connected" ? (link.snapshot?.session ?? null) : null;
  const [onboarded, finishOnboarding] = useOnboarded();

  // A speech engine failed and another stands in for now (VOICE-47): say so wherever the user is.
  useRuntimeEvents((event) => {
    if (event.group === "system" && event.event.type === "speechFallback") toast(event.event.message);
  });

  // A change KIVO can take back (UX-43): the toast offers Undo too, beside the Island's ring.
  const undo = link?.status === "connected" ? link.snapshot?.turn?.undo : undefined;
  const undoUntil = undo?.until;
  const undoTitle = undo?.title;
  useEffect(() => {
    if (!undoUntil || !undoTitle) return;
    toast(t("undo.toast", { title: undoTitle }), {
      onUndo: () => {
        request(Method.sessionUndo).catch((e: unknown) =>
          toast(t("undo.failed", { error: e instanceof Error ? e.message : String(e) })),
        );
      },
    });
  }, [undoUntil, undoTitle, request, t, toast]);

  useEffect(() => {
    if (!palette || link?.status !== "connected") return;
    void request<RoutineView[]>(Method.routinesList)
      .then(setRoutines)
      .catch(() => {});
  }, [palette, link?.status, request]);

  const commands = useMemo<Command[]>(() => {
    const fail = (e: unknown) => toast(e instanceof Error ? e.message : String(e));
    const run = (method: Method) => () => {
      request(method).catch(fail);
    };
    // Every setting, where it lives (UX-47).
    const settings: Command[] = SETTINGS_INDEX.map((s) => ({
      id: `setting-${s.label}`,
      group: t("palette.settingsGroup"),
      label: t(s.label),
      icon: s.icon,
      keywords: s.keywords,
      hint: s.tab ? t(`settings.tabs.${s.tab}`) : t(`nav.${s.page ?? "settings"}`),
      run: () => (s.tab ? navigate(`settings/${s.tab}`) : setPage(s.page ?? "settings")),
    }));
    const runnable: Command[] = routines
      .filter((r) => r.routine.enabled)
      .map((r) => ({
        id: `routine-${r.routine.id}`,
        group: t("palette.routines"),
        label: t("palette.run", { name: r.routine.name }),
        icon: "routine",
        run: () => {
          request(Method.routinesRun, { id: r.routine.id })
            .then(() => toast(t("palette.run", { name: r.routine.name })))
            .catch(fail);
        },
      }));
    const more: Command[] =
      session === null
        ? []
        : [
            {
              id: "remember",
              group: t("palette.actions"),
              label: t("palette.remember"),
              icon: "memory",
              run: () => setPage("memory"),
            },
            {
              id: "new-chat",
              group: t("palette.actions"),
              label: t("palette.newChat"),
              icon: "chat",
              run: () => navigate("chat/new"),
            },
            {
              id: "add-wake",
              group: t("palette.actions"),
              label: t("palette.addWake"),
              icon: "mic",
              run: () => setPage("voice"),
            },
            {
              id: "switch-mode",
              group: t("palette.actions"),
              label: t("palette.switchMode"),
              icon: "permissions",
              run: () => setPage("permissions"),
            },
            ...MODES.map((m): Command => ({
              id: `mode-${m.mode}`,
              group: t("palette.actions"),
              label: t("modes.switchTo", { mode: t(`modes.${m.mode}`) }),
              icon: m.icon,
              keywords: t("modes.keywords"),
              run: () => {
                request(Method.modeSet, { mode: m.mode })
                  .then(() => toast(t("modes.switched", { mode: t(`modes.${m.mode}`) })))
                  .catch(fail);
              },
            })),
            {
              id: "tidy-memory",
              group: t("palette.actions"),
              label: t("palette.tidyMemory"),
              icon: "merge",
              run: run(Method.memoryTidy),
            },
            {
              id: "export-settings",
              group: t("palette.actions"),
              label: t("palette.exportSettings"),
              icon: "upload",
              run: () => {
                request<{ file: string }>(Method.settingsExport)
                  .then((r) => toast(t("settings.general.exported", { file: r.file })))
                  .catch(fail);
              },
            },
            {
              id: "run-checks",
              group: t("palette.actions"),
              label: t("palette.runChecks"),
              icon: "diagnostics",
              run: () => navigate("settings/diagnostics"),
            },
          ];
    const pages: Command[] = [...NAV.flat(), { id: "settings" as const, icon: "settings" as const }].map((item) => ({
      id: `go-${item.id}`,
      group: t("palette.goTo"),
      label: t(`nav.${item.id}`),
      icon: item.icon,
      run: () => setPage(item.id),
    }));
    const actions: Command[] = [];
    if (session === "paused") {
      actions.push({
        id: "resume",
        group: t("palette.kivo"),
        label: t("home.resume"),
        icon: "play",
        run: run(Method.sessionResume),
      });
    } else if (session === "idle" || session === "followUp") {
      actions.push({
        id: "pause",
        group: t("palette.kivo"),
        label: t("home.pause"),
        icon: "pause",
        run: run(Method.sessionPause),
      });
    }
    if (session !== null) {
      actions.push({
        id: "stop-everything",
        group: t("palette.kivo"),
        label: t("home.stopEverything"),
        icon: "stop",
        run: run(Method.sessionStopEverything),
      });
      actions.push({
        id: "quit",
        group: t("palette.kivo"),
        label: t("palette.quit"),
        icon: "power",
        run: run(Method.runtimeQuit),
      });
    }
    return [
      ...actions,
      ...more,
      ...runnable,
      ...pages,
      ...settings,
      {
        id: "gallery",
        group: t("palette.goTo"),
        label: t("palette.gallery"),
        icon: "layers",
        run: () => setPage("gallery"),
      },
    ];
  }, [navigate, request, routines, session, setPage, t, toast]);

  if (onboarded === false) {
    return (
      <AppWindow>
        <Onboarding
          onFinish={(then) => {
            finishOnboarding();
            if (then) navigate(then);
            else setPage("home");
          }}
        />
      </AppWindow>
    );
  }

  return (
    <>
      <AppWindow
        title={page === "gallery" ? t("palette.gallery") : t(`nav.${page}`)}
        sidebar={
          <Sidebar
            current={page}
            onNavigate={setPage}
            onSearch={() => setPalette(true)}
            footer={<RuntimeStatus onOpen={() => setPage("home")} />}
          />
        }
      >
        {page === "gallery" ? (
          <Gallery />
        ) : page === "home" ? (
          <Home
            onOpenPermissions={() => setPage("permissions")}
            onOpenActivity={() => setPage("activity")}
            onOpenVoice={() => setPage("voice")}
            onOpenTasks={() => setPage("tasks")}
          />
        ) : page === "activity" ? (
          <Activity />
        ) : page === "chat" ? (
          <Chat key={focus?.at} onOpenPermissions={() => setPage("permissions")} />
        ) : page === "tasks" ? (
          <Tasks key={focus?.at} focus={focus?.arg} />
        ) : page === "routines" ? (
          <Routines />
        ) : page === "agents" ? (
          <Agents />
        ) : page === "extensions" ? (
          <Extensions key={focus?.at} initialTab={EXTENSION_TABS.find((x) => x === focus?.arg)} />
        ) : page === "brains" ? (
          <Brains key={focus?.at} initialTab={BRAINS_TABS.find((x) => x === focus?.arg)} onNavigate={navigate} />
        ) : page === "voice" ? (
          <Voice />
        ) : page === "settings" ? (
          <Settings
            key={focus?.at}
            initialTab={SETTINGS_TABS.find((x) => x === focus?.arg)}
            onNavigate={(to) => setPage(isPage(to) ? to : "home")}
          />
        ) : page === "permissions" ? (
          <Permissions key={focus?.at} initialTab={PERMISSIONS_TABS.find((x) => x === focus?.arg)} />
        ) : page === "memory" ? (
          <Memory onOpenAgents={() => setPage("agents")} />
        ) : (
          <Usage />
        )}
      </AppWindow>
      <WhatsNewDialog />
      <CommandPalette
        commands={commands}
        open={palette}
        onOpenChange={setPalette}
        onAsk={(text) => {
          request(Method.sessionSay, { text })
            .then(() => toast(t("palette.asked")))
            .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));
        }}
      />
    </>
  );
}
