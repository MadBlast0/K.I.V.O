import { fireEvent, render, screen } from "@testing-library/react";
import { useState } from "react";
import { describe, expect, it, vi } from "vitest";
import { CommandPalette, type Command } from "./CommandPalette";

function Harness({ commands }: { commands: Command[] }) {
  const [open, setOpen] = useState(true);
  return (
    <>
      <span data-testid="state">{open ? "open" : "closed"}</span>
      <CommandPalette commands={commands} open={open} onOpenChange={setOpen} />
    </>
  );
}

const make = () => {
  const home = vi.fn<() => void>();
  const settings = vi.fn<() => void>();
  const voice = vi.fn<() => void>();
  const commands: Command[] = [
    { id: "home", group: "Go to", label: "Home", icon: "home", run: home },
    { id: "settings", group: "Go to", label: "Settings", icon: "settings", run: settings },
    { id: "voice", group: "Voice", label: "Voice settings", icon: "voice", run: voice },
  ];
  return { commands, home, settings, voice };
};

describe("CommandPalette", () => {
  it("filters by label and shows each group header once", () => {
    const { commands } = make();
    render(<Harness commands={commands} />);
    const input = screen.getByRole("combobox");
    fireEvent.change(input, { target: { value: "settings" } });
    expect(screen.getAllByRole("option").map((o) => o.textContent)).toEqual(["Settings", "Voice settings"]);
    expect(screen.getByText("Go to")).toBeTruthy();
    expect(screen.getByText("Voice")).toBeTruthy();
  });

  it("moves the highlight with the arrow keys and runs the highlighted command on Enter", () => {
    const { commands, home, settings } = make();
    render(<Harness commands={commands} />);
    const input = screen.getByRole("combobox");
    fireEvent.keyDown(input, { key: "ArrowDown" });
    expect(input.getAttribute("aria-activedescendant")).toBe(screen.getAllByRole("option")[1]?.id);
    fireEvent.keyDown(input, { key: "Enter" });
    expect(settings).toHaveBeenCalledOnce();
    expect(home).not.toHaveBeenCalled();
    expect(screen.getByTestId("state").textContent).toBe("closed");
  });

  it("does not move past the first or last result", () => {
    const { commands, voice } = make();
    render(<Harness commands={commands} />);
    const input = screen.getByRole("combobox");
    for (let i = 0; i < 10; i++) fireEvent.keyDown(input, { key: "ArrowDown" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(voice).toHaveBeenCalledOnce();
  });

  it("offers to ask KIVO when nothing matches", () => {
    const { commands } = make();
    render(<Harness commands={commands} />);
    fireEvent.change(screen.getByRole("combobox"), { target: { value: "what's the weather" } });
    expect(screen.queryAllByRole("option")).toHaveLength(0);
    expect(screen.getByText("Ask KIVO")).toBeTruthy();
  });

  it("runs a command when it is clicked", () => {
    const { commands, home } = make();
    render(<Harness commands={commands} />);
    fireEvent.click(screen.getByRole("option", { name: "Home" }));
    expect(home).toHaveBeenCalledOnce();
  });
});
