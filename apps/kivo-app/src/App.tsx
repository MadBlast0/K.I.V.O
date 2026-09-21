import { useCallback, useMemo, useState } from "react";
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
import { Home } from "./pages/Home";

const TITLES: Record<PageId, string> = {
  home: "Home",
  chat: "Chat",
  tasks: "Tasks",
  activity: "Activity",
  routines: "Routines",
  brains: "Brains",
  agents: "Agents",
  voice: "Voice",
  extensions: "Extensions",
  permissions: "Permissions",
  memory: "Memory",
  usage: "Usage",
  settings: "Settings",
  gallery: "Components",
};

function isPage(page: string): page is PageId {
  return page in TITLES;
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
  const { link, request } = useRuntime();
  const toast = useToast();
  const session = link?.status === "connected" ? (link.snapshot?.session ?? null) : null;

  const commands = useMemo<Command[]>(() => {
    const run = (method: Method) => () => {
      request(method).catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));
    };
    const pages: Command[] = [
      ...NAV.flat(),
      { id: "settings" as const, label: "Settings", icon: "settings" as const },
    ].map((item) => ({
      id: `go-${item.id}`,
      group: "Go to",
      label: item.label,
      icon: item.icon,
      run: () => setPage(item.id),
    }));
    const actions: Command[] = [];
    if (session === "paused") {
      actions.push({
        id: "resume",
        group: "KIVO",
        label: "Resume listening",
        icon: "play",
        run: run(Method.sessionResume),
      });
    } else if (session === "idle" || session === "followUp") {
      actions.push({
        id: "pause",
        group: "KIVO",
        label: "Pause listening",
        icon: "pause",
        run: run(Method.sessionPause),
      });
    }
    if (session !== null) {
      actions.push({ id: "quit", group: "KIVO", label: "Quit KIVO", icon: "power", run: run(Method.runtimeQuit) });
    }
    return [
      ...actions,
      ...pages,
      { id: "gallery", group: "Go to", label: "Component gallery", icon: "layers", run: () => setPage("gallery") },
    ];
  }, [request, session, setPage, toast]);

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
          <Home />
        ) : (
          <>
            <PageHeader title={TITLES[page]} />
            <EmptyState icon="layers" title="Not built yet">
              This page comes next. The component gallery has every building block.
            </EmptyState>
          </>
        )}
      </AppWindow>
      <CommandPalette commands={commands} open={palette} onOpenChange={setPalette} />
    </>
  );
}
