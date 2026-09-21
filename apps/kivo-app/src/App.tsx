import { useMemo, useState } from "react";
import { AppWindow, PageHeader, Sidebar, type PageId } from "./components/layout/Shell";
import { CommandPalette, EmptyState, ToastProvider, TooltipProvider, useCommandPaletteHotkey, type Command } from "./components/ui";
import { ThemeProvider } from "./lib/theme";
import { Gallery } from "./pages/Gallery";

export function App() {
  return (
    <ThemeProvider>
      <TooltipProvider>
        <ToastProvider>
          <Shell />
        </ToastProvider>
      </TooltipProvider>
    </ThemeProvider>
  );
}

const TITLES: Record<PageId, string> = {
  home: "Home", chat: "Chat", tasks: "Tasks", activity: "Activity", routines: "Routines", brains: "Brains",
  agents: "Agents", voice: "Voice", extensions: "Extensions", permissions: "Permissions", memory: "Memory",
  usage: "Usage", settings: "Settings", gallery: "Components",
};

function Shell() {
  // Pages are built next; until then the app opens on the component gallery.
  const [page, setPage] = useState<PageId>("gallery");
  const [palette, setPalette] = useState(false);
  useCommandPaletteHotkey(setPalette);

  const commands = useMemo<Command[]>(() => [
    { id: "gallery", group: "Go to", label: "Component gallery", icon: "layers", run: () => setPage("gallery") },
    { id: "home", group: "Go to", label: "Home", icon: "home", run: () => setPage("home") },
    { id: "settings", group: "Go to", label: "Settings", icon: "settings", run: () => setPage("settings") },
    { id: "permissions", group: "Go to", label: "Permissions", icon: "permissions", run: () => setPage("permissions") },
  ], []);

  return (
    <>
      <AppWindow sidebar={<Sidebar current={page} onNavigate={setPage} onSearch={() => setPalette(true)} />}>
        {page === "gallery" ? <Gallery /> : (
          <>
            <PageHeader title={TITLES[page]} />
            <EmptyState icon="layers" title="Not built yet">This page comes next. The component gallery has every building block.</EmptyState>
          </>
        )}
      </AppWindow>
      <CommandPalette commands={commands} open={palette} onOpenChange={setPalette} />
    </>
  );
}
