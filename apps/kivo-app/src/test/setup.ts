// Browser APIs that jsdom doesn't implement but KIVO's components use.
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
// Components read their words from the catalog; loading it registers it with react-i18next.
import i18n from "../i18n";

if (!i18n.isInitialized) throw new Error("the translations failed to load");

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.removeAttribute("data-motion");
});

if (!window.matchMedia) {
  window.matchMedia = (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    addListener: () => {},
    removeListener: () => {},
    dispatchEvent: () => false,
  });
}

class ResizeObserverStub implements ResizeObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}
if (typeof globalThis.ResizeObserver === "undefined") globalThis.ResizeObserver = ResizeObserverStub;
