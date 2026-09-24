import { describe, expect, it } from "vitest";
import type { ActivityItem } from "../ipc/generated";
import { entries } from "./activity";

const row = (
  id: number,
  turnId: string | null,
  kind: string,
  title: string,
  status = "done",
  detail: string | null = null,
): ActivityItem => ({
  id,
  ts: 1000 + id,
  turnId,
  kind,
  title,
  detail,
  status,
});

describe("entries", () => {
  it("puts a request's pieces back together, newest request first", () => {
    const items = [
      row(6, "t2", "reply", "Muted."),
      row(5, "t2", "tool", "audio.mute", "done", "muted=true"),
      row(4, "t2", "transcript", "mute"),
      row(3, null, "setting", "Shell commands turned on"),
      row(2, "t1", "reply", "Opening Google Chrome."),
      row(1, "t1", "transcript", "open chrome"),
    ];
    const list = entries(items);
    expect(list.map((e) => e.title)).toEqual(["mute", "Shell commands turned on", "open chrome"]);
    expect(list[0]).toMatchObject({ kind: "tool", outcome: "done", detail: "muted=true — Muted.", ts: 1004 });
    expect(list[1]?.kind).toBe("setting");
    expect(list[2]).toMatchObject({ kind: "voice", detail: "Opening Google Chrome." });
  });

  it("reports the failure when a step failed or was refused", () => {
    const list = entries([
      row(3, "t1", "reply", "Shell commands is off. Turn it on?", "denied"),
      row(2, "t1", "tool", "Run a command", "denied", "Shell commands is off."),
      row(1, "t1", "transcript", "run tests"),
    ]);
    expect(list[0]?.outcome).toBe("denied");
  });

  it("marks the requests a brain answered, for the AI filter", () => {
    const list = entries([
      row(5, "t2", "reply", "It's about 18 degrees."),
      row(4, "t2", "brain", "Needs a brain: a question"),
      row(3, "t2", "transcript", "how warm is it"),
      row(2, "t1", "tool", "audio.mute"),
      row(1, "t1", "transcript", "mute"),
    ]);
    expect(list.map((e) => [e.title, e.ai])).toEqual([
      ["how warm is it", true],
      ["mute", false],
    ]);
  });
});
