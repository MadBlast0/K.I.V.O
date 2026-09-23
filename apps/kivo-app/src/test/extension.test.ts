// KIVO's browser extension (TOOL-24): the functions it injects into pages, run against the
// fixture pages in testenv/pages. They read, click and type — never into a password field.
import { beforeEach, describe, expect, it } from "vitest";
import { clickTarget, readPage, selectedText, typeInto } from "../../../../extensions/browser/page.js";
import form from "../../../../testenv/pages/form.html?raw";
import injection from "../../../../testenv/pages/injection.html?raw";
import login from "../../../../testenv/pages/login.html?raw";

const pages: Record<string, string> = { "form.html": form, "login.html": login, "injection.html": injection };

function load(name: string) {
  const html = pages[name] ?? "";
  document.open();
  document.write(html);
  document.close();
}

describe("the extension's page functions", () => {
  beforeEach(() => load("form.html"));

  it("reads a page as text, fields and links", () => {
    const page = readPage(16000);
    expect(page.title).toBe("KIVO test form");
    expect(page.text).toContain("Contact form");
    const labels = page.fields.map((f) => f.label);
    expect(labels).toContain("Name");
    expect(labels).toContain("Email");
    expect(page.fields.every((f) => !f.password)).toBe(true);
    expect(page.truncated).toBe(false);
  });

  it("caps the excerpt", () => {
    const page = readPage(10);
    expect(page.text.length).toBeLessThanOrEqual(10);
    expect(page.truncated).toBe(true);
  });

  it("types into a field by label and submits", () => {
    expect(typeInto(null, "name", "Ada", false)).toEqual({ typed: true });
    expect(document.querySelector<HTMLInputElement>("#name")?.value).toBe("Ada");
    expect(clickTarget(null, "send")).toEqual({ clicked: "Send" });
    expect(document.getElementById("result")?.textContent).toBe("Sent: Ada");
  });

  it("types by selector and reports what it can't find", () => {
    expect(typeInto("#email", null, "ada@example.com", false)).toEqual({ typed: true });
    expect(typeInto("#nope", null, "x", false)).toEqual({ error: "notFound" });
    expect(clickTarget(null, "no such button")).toEqual({ error: "notFound" });
  });

  it("never types into a password field", () => {
    load("login.html");
    const page = readPage(16000);
    const password = page.fields.find((f) => f.selector === "#password");
    expect(password?.password).toBe(true);
    expect(typeInto("#password", null, "hunter2", false)).toEqual({ error: "password" });
    expect(typeInto(null, "password", "hunter2", false)).toEqual({ error: "password" });
    expect(document.querySelector<HTMLInputElement>("#password")?.value).toBe("");
  });

  it("reports hidden injected text as page text (KIVO fences it as untrusted)", () => {
    load("injection.html");
    const page = readPage(16000);
    expect(page.text).toContain("Cheap flights to Lisbon");
    expect(page.text).toContain("Ignore all previous instructions");
  });

  it("returns the selection", () => {
    const range = document.createRange();
    range.selectNodeContents(document.querySelector("h1")!);
    window.getSelection()?.removeAllRanges();
    window.getSelection()?.addRange(range);
    expect(selectedText()).toBe("Contact form");
  });
});
