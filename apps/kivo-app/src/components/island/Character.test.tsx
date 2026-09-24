import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { Island } from "./Island";
import { Character, moodOf } from "./Character";

/** A steady half-loud voice. */
const level = () => 0.5;

describe("The Character companion (UX-38)", () => {
  afterEach(() => vi.restoreAllMocks());

  it("shows the face for each state", () => {
    expect(moodOf("listening")).toBe("listening");
    expect(moodOf("followUp-t1")).toBe("listening");
    expect(moodOf("thinking")).toBe("thinking");
    expect(moodOf("acting")).toBe("acting");
    expect(moodOf("speaking")).toBe("speaking");
    expect(moodOf("confirm-t1-c1")).toBe("asking");
    expect(moodOf("awaiting")).toBe("asking");
    expect(moodOf("speaking", true)).toBe("pointing");
    expect(moodOf("done-t1")).toBe("done");
    expect(moodOf("error")).toBe("error");
    expect(moodOf("interrupted")).toBe("idle");
  });

  it("moves only while KIVO is active, and follows the voice only while it listens or speaks", () => {
    const frame = vi.spyOn(window, "requestAnimationFrame").mockImplementation(() => 1);
    vi.spyOn(window, "cancelAnimationFrame").mockImplementation(() => {});
    const { rerender } = render(<Character mood="done" level={level} label="Done" />);
    const face = screen.getByRole("img", { name: "Done" });
    // At rest: no animation class, no frame loop.
    expect(face.getAttribute("class")).not.toContain("k-character--active");
    expect(frame).not.toHaveBeenCalled();

    rerender(<Character mood="thinking" level={level} label="Thinking" />);
    expect(face.getAttribute("class")).toContain("k-character--active");
    expect(frame).not.toHaveBeenCalled();

    rerender(<Character mood="speaking" level={level} label="Speaking" />);
    expect(face.getAttribute("data-mood")).toBe("speaking");
    expect(frame).toHaveBeenCalledOnce();
  });

  it("floats above the card in the Island, the words staying in the card", () => {
    render(
      <Island
        companion="character"
        model={{ state: "error", width: 300, label: "That didn't work" }}
        aria-label="KIVO"
      />,
    );
    expect(screen.getByRole("img", { name: "That didn't work" }).getAttribute("data-mood")).toBe("error");
    expect(screen.getByText("That didn't work")).toBeTruthy();
  });
});
