import { act, fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { AccentPicker, ShortcutRecorder } from "./Pickers";

describe("ShortcutRecorder", () => {
  it("records modifiers plus a key, and reports conflicts", () => {
    const onChange = vi.fn<(keys: string[]) => void>();
    render(
      <ShortcutRecorder
        value={["Ctrl", "Space"]}
        onChange={onChange}
        conflict={(k) => (k.join("+") === "Ctrl+Shift+K" ? "Used by Slack" : null)}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Change" }));
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Shift", ctrlKey: true, shiftKey: true }));
    });
    expect(onChange).not.toHaveBeenCalled(); // modifiers alone don't finish the recording
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "k", ctrlKey: true, shiftKey: true }));
    });
    expect(onChange).toHaveBeenCalledWith(["Ctrl", "Shift", "K"]);
    expect(screen.getByText("Used by Slack")).toBeTruthy();
  });

  it("cancels on Escape without changing the shortcut", () => {
    const onChange = vi.fn<(keys: string[]) => void>();
    render(<ShortcutRecorder value={["Ctrl", "Space"]} onChange={onChange} />);
    fireEvent.click(screen.getByRole("button", { name: "Change" }));
    act(() => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(onChange).not.toHaveBeenCalled();
    expect(screen.getByRole("button", { name: "Change" })).toBeTruthy();
  });
});

describe("AccentPicker", () => {
  it("marks the current accent and reports a new choice", () => {
    const onChange = vi.fn<(accent: string) => void>();
    render(<AccentPicker value="blue" onChange={onChange} />);
    expect(screen.getByRole("radio", { name: "Blue" }).getAttribute("aria-checked")).toBe("true");
    fireEvent.click(screen.getByRole("radio", { name: "Teal" }));
    expect(onChange).toHaveBeenCalledWith("teal");
  });
});
