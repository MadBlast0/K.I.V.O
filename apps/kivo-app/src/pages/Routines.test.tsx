import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "../components/ui";
import type { Routine, RoutineCheck, RoutineView, ToolItem } from "../ipc/generated";
import { fieldsOf, labelOf, missing, parseInput } from "../lib/schemaForm";

const calls: Array<{ method: string; params: unknown }> = [];
let routines: RoutineView[] = [];
let check: RoutineCheck = { collisions: [], grants: [], problems: [] };

const tools: ToolItem[] = [
  {
    id: "apps.launch",
    title: "Open an app",
    description: "Open an app by name.",
    params: {
      type: "object",
      properties: { app: { type: "string", description: "The app's name" }, maximized: { type: "boolean" } },
      required: ["app"],
    },
    risk: "low",
    capability: "apps-and-windows",
  },
];

const runtime = vi.hoisted(() => ({
  link: { status: "connected", runtimeVersion: "0.0.0", snapshot: { mode: "auto" }, message: null },
  request: (_method: string, _params?: unknown): Promise<unknown> => Promise.resolve(null),
}));

vi.mock("../ipc/runtime", () => ({
  useRuntime: () => runtime,
  useRuntimeEvents: () => {},
}));

/** A routine waiting for review (ROUT-13), taken once. */
let draft: Routine | null = null;

interface Saved {
  routine: Routine;
  grant?: boolean;
}

function isSaved(v: unknown): v is Saved {
  return typeof v === "object" && v !== null && "routine" in v;
}

function view(routine: Routine, over: Partial<RoutineView> = {}): RoutineView {
  return { routine, containsAi: false, customCommand: false, granted: true, lastRun: null, updatedAt: 1, ...over };
}

runtime.request = (method: string, params?: unknown) => {
  calls.push({ method, params });
  switch (method) {
    case "routines.list":
      return Promise.resolve(routines);
    case "routines.tools":
      return Promise.resolve(tools);
    case "routines.check":
      return Promise.resolve(check);
    case "routines.save": {
      if (!isSaved(params)) return Promise.reject(new Error("bad params"));
      const saved = view({ ...params.routine, id: params.routine.id || "r-new" });
      routines = [...routines.filter((r) => r.routine.id !== saved.routine.id), saved];
      return Promise.resolve(saved);
    }
    case "routines.draft": {
      const d = draft;
      draft = null;
      return Promise.resolve(d);
    }
    case "routines.import":
      return Promise.resolve({ ...blankRoutine(), name: "Imported" });
    case "routines.export":
      return Promise.resolve({ file: "C:/Users/Sam/Downloads/Work mode.kivo-routine.json" });
    default:
      return Promise.resolve(null);
  }
};

const { Routines, moveStep, withTriggers, blankRoutine, phrasesOf, hotkeyOf } = await import("./Routines");

const settle = () =>
  act(async () => {
    await new Promise((r) => setTimeout(r, 300));
  });

const workMode: Routine = {
  id: "r1",
  name: "Work mode",
  description: "Open apps and say ready",
  enabled: true,
  triggers: [
    { type: "phrase", phrases: ["work mode"], lang: "en" },
    { type: "hotkey", chord: "Ctrl+Alt+W" },
  ],
  steps: [
    { id: "a", action: { type: "tool", tool: "apps.launch", args: { app: "Slack" } }, onError: { policy: "stop" } },
    { id: "b", action: { type: "say", text: "Ready." }, onError: { policy: "continue" } },
  ],
  variables: [],
  grants: [],
};

beforeEach(() => {
  calls.length = 0;
  routines = [];
  check = { collisions: [], grants: [], problems: [] };
});

function page() {
  render(
    <ToastProvider>
      <Routines />
    </ToastProvider>,
  );
}

describe("Routines page (UX-26)", () => {
  it("lists routines with their triggers and AI badge, runs, and switches them", async () => {
    routines = [
      view(workMode),
      view(
        {
          ...workMode,
          id: "r2",
          name: "Morning brief",
          triggers: [{ type: "phrase", phrases: ["good morning"], lang: "en" }],
        },
        { containsAi: true },
      ),
    ];
    page();
    await settle();
    expect(screen.getByText(/“work mode” · Ctrl\+Alt\+W/)).toBeTruthy();
    expect(screen.getByText(/uses AI/)).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Run “Work mode”" }));
    await settle();
    expect(calls).toContainEqual({ method: "routines.run", params: { id: "r1", vars: {} } });
    fireEvent.click(screen.getByRole("switch", { name: "“Work mode” on" }));
    await settle();
    expect(calls).toContainEqual({ method: "routines.enable", params: { id: "r1", on: false } });
  });

  it("asks for a routine's variables before running it", async () => {
    routines = [view({ ...workMode, id: "f", name: "Focus", variables: [{ name: "minutes", kind: "number" }] })];
    page();
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Run “Focus”" }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.change(within(dialog).getByRole("textbox", { name: "minutes" }), { target: { value: "25" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Run" }));
    await settle();
    expect(calls).toContainEqual({ method: "routines.run", params: { id: "f", vars: { minutes: "25" } } });
  });

  it("builds a routine: phrase, a tool step from its schema, a say step, check, then saves with its grants (ROUT-09, ROUT-03)", async () => {
    check = {
      collisions: [{ phrase: "start", kind: "command", with: "a built-in command", blocking: false }],
      grants: [
        { tool: "apps.launch", title: "Open Slack", risk: "low", capability: "apps-and-windows", capabilityOff: false },
      ],
      problems: [],
    };
    page();
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "New routine" }));
    fireEvent.click(await screen.findByRole("button", { name: "Start from scratch" }));
    const builder = await screen.findByRole("region", { name: "Routine builder" });
    fireEvent.change(within(builder).getByRole("textbox", { name: "Name" }), { target: { value: "Start" } });
    fireEvent.change(within(builder).getByRole("textbox", { name: "Add phrase" }), { target: { value: "start" } });
    fireEvent.click(within(builder).getByRole("button", { name: "Add phrase" }));
    fireEvent.click(within(builder).getByRole("button", { name: "Action" }));
    fireEvent.change(within(builder).getByRole("textbox", { name: "App" }), { target: { value: "Slack" } });
    fireEvent.click(within(builder).getByRole("button", { name: "Say" }));
    fireEvent.change(within(builder).getByRole("textbox", { name: "Say something" }), { target: { value: "Ready." } });
    await settle();
    // The check ran on the draft and its warning shows.
    expect(calls.some((c) => c.method === "routines.check")).toBe(true);
    expect(within(builder).getByText("“start” sounds like a built-in command")).toBeTruthy();
    expect(within(builder).getByText(/Allowed for this routine: Open Slack/)).toBeTruthy();
    // Reorder: the say step first.
    fireEvent.click(within(builder).getByRole("button", { name: "Move step 2 up" }));
    fireEvent.click(within(builder).getByRole("button", { name: "Save" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Open Slack")).toBeTruthy();
    fireEvent.click(within(dialog).getByRole("button", { name: "Allow and save" }));
    await settle();
    const sent = calls.find((c) => c.method === "routines.save")?.params;
    if (!isSaved(sent)) throw new Error("routines.save wasn't sent");
    expect(sent.grant).toBe(true);
    expect(sent.routine.name).toBe("Start");
    expect(phrasesOf(sent.routine)).toEqual(["start"]);
    expect(sent.routine.steps.map((s) => s.action.type)).toEqual(["say", "tool"]);
    expect(sent.routine.steps[1]?.action).toEqual({ type: "tool", tool: "apps.launch", args: { app: "Slack" } });
  });

  it("won't save while a phrase clashes (ROUT-07)", async () => {
    check = {
      collisions: [{ phrase: "mute", kind: "command", with: "mute", blocking: true }],
      grants: [],
      problems: [],
    };
    routines = [view(workMode)];
    page();
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Edit “Work mode”" }));
    await settle();
    const builder = screen.getByRole("region", { name: "Routine builder" });
    expect(within(builder).getByText("“mute” is already mute")).toBeTruthy();
    expect(within(builder).getByRole("button", { name: "Save" })).toHaveProperty("disabled", true);
  });
});

describe("Drafts, import and export (ROUT-13, ROUT-14)", () => {
  it("opens a waiting draft, imports a file into the builder and exports a saved routine", async () => {
    check = { collisions: [], grants: [], problems: [] };
    routines = [view(workMode)];
    draft = { ...blankRoutine(), name: "Set the volume to 30" };
    page();
    await settle();
    let builder = await screen.findByRole("region", { name: "Routine builder" });
    expect(within(builder).getByRole("textbox", { name: "Name" })).toHaveProperty("value", "Set the volume to 30");
    expect(calls.some((c) => c.method === "routines.save")).toBe(false);
    fireEvent.click(within(builder).getByRole("button", { name: "Close" }));

    const file = new File(['{"kind":"kivo-routine","version":1,"routine":{}}'], "Work.kivo-routine.json");
    fireEvent.change(screen.getByLabelText("Import a routine file"), { target: { files: [file] } });
    await settle();
    await settle();
    expect(calls).toContainEqual({
      method: "routines.import",
      params: { content: '{"kind":"kivo-routine","version":1,"routine":{}}' },
    });
    builder = await screen.findByRole("region", { name: "Routine builder" });
    expect(within(builder).getByRole("textbox", { name: "Name" })).toHaveProperty("value", "Imported");

    fireEvent.click(screen.getByRole("button", { name: "Edit “Work mode”" }));
    await settle();
    builder = screen.getByRole("region", { name: "Routine builder" });
    fireEvent.click(within(builder).getByRole("button", { name: "Export" }));
    await settle();
    expect(calls).toContainEqual({ method: "routines.export", params: { id: workMode.id } });
  });
});

describe("Schedules and events (ROUT-11)", () => {
  it("adds a time of day, says it runs unattended, and keeps the phrases when saving", async () => {
    check = { collisions: [], grants: [], problems: [] };
    routines = [view(workMode)];
    page();
    await settle();
    fireEvent.click(screen.getByRole("button", { name: "Edit “Work mode”" }));
    await settle();
    const builder = screen.getByRole("region", { name: "Routine builder" });
    fireEvent.click(within(builder).getByRole("button", { name: "Add a trigger" }));
    const dialog = await screen.findByRole("dialog", { name: "Add a trigger" });
    fireEvent.change(within(dialog).getByLabelText("Time"), { target: { value: "08:15" } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Fri" }));
    fireEvent.click(within(dialog).getByRole("button", { name: "Add" }));
    await settle();
    expect(within(builder).getByText("Mon, Tue, Wed, Thu at 08:15")).toBeTruthy();
    expect(within(builder).getByText(/ask on screen first/)).toBeTruthy();
    fireEvent.click(within(builder).getByRole("button", { name: "Save" }));
    const confirm = await screen.findByRole("dialog");
    fireEvent.click(within(confirm).getByRole("button", { name: "Allow and save" }));
    await settle();
    const sent = calls.findLast((c) => c.method === "routines.save")?.params;
    if (!isSaved(sent)) throw new Error("routines.save wasn't sent");
    expect(phrasesOf(sent.routine)).toEqual(phrasesOf(workMode));
    expect(sent.routine.triggers).toContainEqual({
      type: "event",
      event: { kind: "timeOfDay", time: "08:15", days: [1, 2, 3, 4] },
    });
  });
});

describe("routine helpers", () => {
  it("moves steps and sets triggers", () => {
    const steps = workMode.steps;
    expect(moveStep(steps, 1, 0).map((s) => s.id)).toEqual(["b", "a"]);
    expect(moveStep(steps, 0, 5)).toBe(steps);
    const r = withTriggers(blankRoutine(), [], "Ctrl+Alt+F");
    expect(hotkeyOf(r)).toBe("Ctrl+Alt+F");
    expect(withTriggers(blankRoutine(), [], undefined).triggers).toEqual([{ type: "manual" }]);
    // Schedules and events stay when the phrases or hotkey change.
    const scheduled = { ...blankRoutine(), triggers: [{ type: "schedule" as const, cron: "@daily", tz: null }] };
    expect(withTriggers(scheduled, ["go"], undefined).triggers).toEqual([
      { type: "phrase", phrases: ["go"], lang: "en" },
      { type: "schedule", cron: "@daily", tz: null },
    ]);
  });
});

describe("forms from JSON Schema (ROUT-09)", () => {
  const schema = {
    type: "object",
    properties: {
      path: { type: "string" },
      level: { type: "integer", description: "0–100" },
      mode: { type: "string", enum: ["default", "bypass"] },
      quiet: { type: ["boolean", "null"] },
      region: { type: "object" },
    },
    required: ["level"],
  };
  it("makes one field per property, required first", () => {
    const fields = fieldsOf(schema);
    expect(fields.map((f) => [f.name, f.kind])).toEqual([
      ["level", "number"],
      ["path", "text"],
      ["mode", "choice"],
      ["quiet", "boolean"],
      ["region", "json"],
    ]);
    expect(fields[0]?.description).toBe("0–100");
    expect(labelOf("file_name")).toBe("File name");
    expect(labelOf("maxResults")).toBe("Max results");
  });
  it("parses what was typed, keeping phrase variables", () => {
    const [level, , , , region] = fieldsOf(schema);
    if (!level || !region) throw new Error("fields missing");
    expect(parseInput(level, "30")).toBe(30);
    expect(parseInput(level, "{minutes}")).toBe("{minutes}");
    expect(parseInput(level, "")).toBeUndefined();
    expect(parseInput(region, '{"x": 1}')).toEqual({ x: 1 });
    expect(() => parseInput(region, "{")).toThrow(SyntaxError);
    expect(missing(fieldsOf(schema), {})).toEqual(["level"]);
    expect(missing(fieldsOf(schema), { level: 3 })).toEqual([]);
  });
});
