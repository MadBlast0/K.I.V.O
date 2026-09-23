import { useCallback, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { RuntimeStatus } from "./components/layout/RuntimeStatus";
import { AppWindow, NAV, PageHeader, Sidebar, type PageId } from "./components/layout/Shell";
import {
  CommandPalette,
  EmptyState,
  ToastProvider,
  TooltipProvider,
  useCommandPaletteHotkey,
  useToast,
  type Command,
} from "./components/ui";
import { Method } from "./ipc/generated";
import { RuntimeProvider, useRuntime } from "./ipc/runtime";
import { ThemeProvider } from "./lib/theme";
import { Gallery } from "./pages/Gallery";
import { Activity } from "./pages/Activity";
import { Voice } from "./pages/Voice";
import { Home } from "./pages/Home";

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
  // The runtime asks for pages by name (tray "Settings", `--page`); unknown names open Home.
  const navigate = useCallback((to: string) => setPage(isPage(to) ? to : "home"), []);
  return (
    <ThemeProvider>
      <TooltipProvider>
        <ToastProvider>
          <RuntimeProvider onNavigate={navigate}>
            <Shell page={page} setPage={setPage} />
          </RuntimeProvider>
        </ToastProvider>
      </TooltipProvider>
    </ThemeProvider>
  );
}

function Shell({ page, setPage }: { page: PageId; setPage: (page: PageId) => void }) {
  const [palette, setPalette] = useState(false);
  useCommandPaletteHotkey(setPalette);
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const session = link?.status === "connected" ? (link.snapshot?.session ?? null) : null;

  const commands = useMemo<Command[]>(() => {
    const run = (method: Method) => () => {
      request(method).catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));
    };
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
      ...pages,
      {
        id: "gallery",
        group: t("palette.goTo"),
        label: t("palette.gallery"),
        icon: "layers",
        run: () => setPage("gallery"),
      },
    ];
  }, [request, session, setPage, t, toast]);

  return (
    <>
      <AppWindow
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
          />
        ) : page === "activity" ? (
          <Activity />
        ) : page === "voice" ? (
          <Voice />
        ) : (
          <>
            <PageHeader title={t(`nav.${page}`)} />
            <EmptyState icon="layers" title={t("shell.notBuilt")}>
              {t("shell.notBuiltDetail")}
            </EmptyState>
          </>
        )}
      </AppWindow>
      <CommandPalette commands={commands} open={palette} onOpenChange={setPalette} />
    </>
  );
}
