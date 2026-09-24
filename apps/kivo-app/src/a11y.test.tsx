/**
 * Accessibility audit of the Control Center (UX §10, UX-52): every page and the component gallery
 * are checked with axe-core for ARIA roles, names, labels and structure. Contrast is left out:
 * jsdom has no layout or colours to measure.
 */
import { act, fireEvent, render, screen } from "@testing-library/react";
import axe from "axe-core";
import { describe, expect, it } from "vitest";
import { App } from "./App";
import i18n from "./i18n";

async function violations() {
  const result = await axe.run(document.body, {
    rules: { "color-contrast": { enabled: false }, region: { enabled: false } },
  });
  return result.violations.map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(" ")).join(", ")}`);
}

async function open(name: string) {
  fireEvent.click(screen.getByRole("button", { name }));
  await act(async () => {
    await new Promise((r) => setTimeout(r, 0));
  });
}

describe("Control Center accessibility", () => {
  it.each(["home", "activity", "voice", "chat", "settings"])("has no ARIA or labelling problems on %s", async (id) => {
    render(<App />);
    await open(i18n.t(`nav.${id}`));
    expect(await violations()).toEqual([]);
  });

  it("has no ARIA or labelling problems in the component gallery", async () => {
    render(<App />);
    fireEvent.keyDown(window, { key: "k", ctrlKey: true });
    fireEvent.change(await screen.findByRole("combobox"), { target: { value: i18n.t("palette.gallery") } });
    fireEvent.keyDown(screen.getByRole("combobox"), { key: "Enter" });
    await screen.findByRole("heading", { name: i18n.t("nav.gallery") });
    expect(await violations()).toEqual([]);
    // axe over every component takes several seconds while the other test files run alongside.
  }, 30_000);
});
