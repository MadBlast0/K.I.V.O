/**
 * The Control Center in the real app (ARCH-37): every page and Settings tab opens with its heading,
 * the Ctrl+K palette finds a setting, and nothing logs an error. Read-only: no setting is changed,
 * so it's safe against the owner's own KIVO.
 */
/* eslint-disable no-await-in-loop -- UI steps run one after another */
import { chromium, expect, test, type Browser, type Page } from "@playwright/test";

const CDP = process.env.KIVO_CDP ?? "http://127.0.0.1:9223";

let browser: Browser;
let page: Page;
const errors: string[] = [];

test.beforeAll(async () => {
  browser = await chromium.connectOverCDP(CDP);
  // The Control Center, not the Island's overlay window.
  const pages = browser.contexts().flatMap((c) => c.pages());
  const main = pages.find((p) => !p.url().includes("overlay"));
  if (!main) throw new Error(`no Control Center window among ${pages.map((p) => p.url()).join(", ")}`);
  page = main;
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  page.on("pageerror", (e) => errors.push(e.message));
});

test.afterAll(async () => {
  // Disconnect only: the app keeps running.
  await browser.close();
});

const PAGES = [
  "Home",
  "Chat",
  "Tasks",
  "Activity",
  "Routines",
  "Brains",
  "Agents",
  "Voice",
  "Extensions",
  "Permissions",
  "Memory",
  "Usage",
  "Settings",
];

test("every page opens from the sidebar with its title", async () => {
  const nav = page.getByRole("navigation", { name: "Main" });
  for (const name of PAGES) {
    await nav.getByRole("button", { name, exact: true }).click();
    // Home is the Island's big status and Chat fills the window; every other page has its title.
    if (name === "Home") await expect(page.locator(".k-home__title")).toBeVisible();
    else if (name === "Chat") await expect(page.getByRole("complementary", { name: "Conversations" })).toBeVisible();
    else await expect(page.locator("h1.k-page__title")).toHaveText(name);
  }
});

test("every Settings tab opens", async () => {
  await page.getByRole("navigation", { name: "Main" }).getByRole("button", { name: "Settings", exact: true }).click();
  for (const tab of [
    "General",
    "Appearance",
    "Island",
    "Sounds",
    "Notifications",
    "Accessibility",
    "Shortcuts",
    "Performance",
    "Diagnostics",
    "About",
  ]) {
    await page.getByRole("tab", { name: tab, exact: true }).click();
    await expect(page.getByRole("tab", { name: tab, exact: true })).toHaveAttribute("aria-selected", "true");
  }
});

test("the palette finds a setting and opens its tab", async () => {
  await page.keyboard.press("Control+K");
  const input = page.getByRole("combobox");
  await expect(input).toBeVisible();
  await input.fill("accent colour");
  await page.keyboard.press("Enter");
  await expect(page.locator("h1.k-page__title")).toHaveText("Settings");
  await expect(page.getByRole("tab", { name: "Appearance", exact: true })).toHaveAttribute("aria-selected", "true");
});

test("nothing logged an error", () => {
  expect(errors).toEqual([]);
});
