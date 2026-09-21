import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ThemeProvider, useTheme } from "./theme";

/** Drives the provider through buttons, the way the Appearance settings will. */
function Controls() {
  const t = useTheme();
  return (
    <>
      <output data-testid="resolved">{t.resolved}</output>
      <button onClick={() => t.setTheme("dark")}>dark</button>
      <button onClick={() => t.setTheme("system")}>system</button>
      <button onClick={() => t.setAccent("teal")}>teal</button>
      <button onClick={() => t.setTextSize("large")}>large</button>
      <button onClick={() => t.setMotion("reduced")}>reduced</button>
      <button onClick={() => t.setMotion("system")}>motion-system</button>
    </>
  );
}
const mount = () =>
  render(
    <ThemeProvider>
      <Controls />
    </ThemeProvider>,
  );
const press = (name: string) => fireEvent.click(screen.getByRole("button", { name }));
const root = document.documentElement;

describe("ThemeProvider", () => {
  it("defaults to the light theme, blue accent and motion that follows Windows", () => {
    mount();
    expect(root.dataset.theme).toBe("light");
    expect(root.dataset.accent).toBe("blue");
    expect(root.dataset.motion).toBeUndefined();
  });

  it("applies and persists theme, accent, text size and reduced motion", () => {
    mount();
    ["dark", "teal", "large", "reduced"].forEach(press);
    expect(root.dataset.theme).toBe("dark");
    expect(root.dataset.accent).toBe("teal");
    expect(root.dataset.textsize).toBe("large");
    expect(root.dataset.motion).toBe("reduced");
    expect(JSON.parse(localStorage.getItem("kivo.appearance") ?? "{}")).toEqual({
      theme: "dark",
      accent: "teal",
      textSize: "large",
      motion: "reduced",
    });
  });

  it("restores saved settings, and removes the motion attribute when set back", () => {
    localStorage.setItem("kivo.appearance", JSON.stringify({ theme: "dark", motion: "reduced" }));
    mount();
    expect(root.dataset.theme).toBe("dark");
    expect(root.dataset.motion).toBe("reduced");
    press("motion-system");
    expect(root.dataset.motion).toBeUndefined();
  });

  it("resolves System to light when Windows is in light mode", () => {
    mount();
    press("system");
    expect(screen.getByTestId("resolved").textContent).toBe("light");
    expect(root.dataset.theme).toBe("light");
  });
});
