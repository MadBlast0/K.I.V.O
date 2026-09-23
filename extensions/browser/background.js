// KIVO's browser extension (TOOLS_AND_CONTROL §5, TOOL-24). It connects to KIVO through the
// native messaging host (`kivo-runtime --native-messaging`) and answers KIVO's requests about the
// user's own browser: the tabs, a readable excerpt of a page, the selection, and clicking or
// typing on a page. It does nothing on its own, keeps nothing, and never types into a password
// field. Messages are JSON-RPC 2.0, at most 1 MiB each (SEC-30).

import { readPage, selectedText, clickTarget, typeInto } from "./page.js";

const HOST = "com.kivo.bridge";
const MAX_MESSAGE = 1024 * 1024;
const EXCERPT_CHARS = 16000;

let port = null;
let retry = 1000;

function connect() {
  try {
    port = chrome.runtime.connectNative(HOST);
  } catch {
    schedule();
    return;
  }
  port.onMessage.addListener((message) => {
    retry = 1000;
    handle(message).then(
      (result) => reply(message.id, { result }),
      (error) => reply(message.id, { error: { code: -32000, message: String(error?.message || error) } }),
    );
  });
  port.onDisconnect.addListener(() => {
    port = null;
    schedule();
  });
}

// KIVO isn't running (or the host isn't installed): try again later, backing off.
function schedule() {
  setTimeout(connect, retry);
  retry = Math.min(retry * 2, 60000);
}

function reply(id, body) {
  if (!port || id === undefined) return;
  let message = { jsonrpc: "2.0", id, ...body };
  if (JSON.stringify(message).length > MAX_MESSAGE) {
    message = { jsonrpc: "2.0", id, error: { code: -32001, message: "tooLarge" } };
  }
  port.postMessage(message);
}

async function tabFor(id) {
  if (typeof id === "number" && id >= 0) return chrome.tabs.get(id);
  const [active] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  if (!active) throw new Error("noTab");
  return active;
}

async function inPage(tab, func, args) {
  if (!/^https?:/.test(tab.url || "")) throw new Error("notAWebPage");
  const [frame] = await chrome.scripting.executeScript({ target: { tabId: tab.id }, func, args });
  return frame?.result;
}

async function handle(message) {
  if (message?.jsonrpc !== "2.0" || typeof message.method !== "string") throw new Error("badRequest");
  const p = message.params || {};
  switch (message.method) {
    case "ping":
      return { version: chrome.runtime.getManifest().version };
    case "tabs": {
      const tabs = await chrome.tabs.query({});
      return tabs
        .filter((t) => t.url)
        .map((t) => ({ id: t.id, title: t.title || "", url: t.url || "", active: !!t.active && !!t.highlighted }));
    }
    case "read":
      return inPage(await tabFor(p.tab), readPage, [EXCERPT_CHARS]);
    case "selection":
      return inPage(await tabFor(p.tab), selectedText, []);
    case "click": {
      const r = await inPage(await tabFor(p.tab), clickTarget, [p.selector || null, p.text || null]);
      if (r?.error) throw new Error(r.error);
      return r;
    }
    case "type": {
      const r = await inPage(await tabFor(p.tab), typeInto, [
        p.selector || null,
        p.text || null,
        String(p.value ?? ""),
        !!p.submit,
      ]);
      if (r?.error) throw new Error(r.error);
      return r;
    }
    default:
      throw new Error("unknownMethod");
  }
}

connect();
