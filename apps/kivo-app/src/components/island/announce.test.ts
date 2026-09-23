import { describe, expect, it } from "vitest";
import i18n from "../../i18n";
import { NOTHING, announcements, type Heard } from "./announce";

const t = () => i18n.t;

describe("announcements (UX-53)", () => {
  it("says each new state once, then what was heard and the answer", () => {
    const listening: Heard = { ...NOTHING, session: "listening" };
    expect(announcements(NOTHING, listening, t())).toEqual(["Listening"]);
    expect(announcements(listening, listening, t())).toEqual([]);
    const thinking: Heard = { ...listening, session: "thinking", transcript: "Mute" };
    expect(announcements(listening, thinking, t())).toEqual(["Thinking", "You said: Mute"]);
    const done: Heard = { ...thinking, session: "idle", answer: "Muted." };
    expect(announcements(thinking, done, t())).toEqual(["KIVO: Muted."]);
  });

  it("reads an error in its own words instead of the state's name", () => {
    const failed: Heard = { ...NOTHING, session: "error", error: "Chrome isn't installed." };
    expect(announcements(NOTHING, failed, t())).toEqual(["Chrome isn't installed."]);
  });
});
