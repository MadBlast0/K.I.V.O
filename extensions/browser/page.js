// Functions injected into a page with chrome.scripting.executeScript. Each one is serialized and
// run on its own in the page, so it must be self-contained: no imports, no helpers from outside
// its own body. They see only the page and return plain data; nothing here sends anything
// anywhere. KIVO treats everything a page says as untrusted.

/** A readable excerpt: the main text, the form fields (passwords flagged) and the first links. */
export function readPage(maxChars) {
  const txt = (el) => (el.innerText ?? el.textContent ?? "").trim();
  const esc = (s) => (globalThis.CSS && CSS.escape ? CSS.escape(s) : String(s).replace(/["\\#.:]/g, "\\$&"));
  const root =
    document.querySelector("main, article, [role=main]") || document.body || document.documentElement;
  const skip = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEMPLATE", "SVG", "CANVAS", "IFRAME"]);
  const parts = [];
  let length = 0;
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
    acceptNode(node) {
      for (let el = node.parentElement; el; el = el.parentElement) {
        if (skip.has(el.tagName)) return NodeFilter.FILTER_REJECT;
      }
      return node.textContent.trim() ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_SKIP;
    },
  });
  while (walker.nextNode() && length < maxChars) {
    const text = walker.currentNode.textContent.replace(/\s+/g, " ").trim();
    parts.push(text);
    length += text.length + 1;
  }
  let text = parts.join("\n");
  const truncated = text.length > maxChars;
  if (truncated) text = text.slice(0, maxChars);

  const label = (el) => {
    if (el.id) {
      const l = document.querySelector(`label[for="${esc(el.id)}"]`);
      if (l) return txt(l);
    }
    const wrapping = el.closest("label");
    if (wrapping) return txt(wrapping);
    return (el.getAttribute("aria-label") || el.getAttribute("placeholder") || el.name || "").trim();
  };
  const selectorOf = (el) => {
    if (el.id) return `#${esc(el.id)}`;
    if (el.name) return `${el.tagName.toLowerCase()}[name="${esc(el.name)}"]`;
    const all = Array.from(document.querySelectorAll(el.tagName));
    return `${el.tagName.toLowerCase()}:nth-of-type(${all.indexOf(el) + 1})`;
  };
  const fields = Array.from(document.querySelectorAll("input, textarea, select, button"))
    .filter((el) => el.type !== "hidden")
    .slice(0, 60)
    .map((el) => ({
      selector: selectorOf(el),
      label: label(el) || (el.tagName === "BUTTON" ? txt(el) : ""),
      kind: (el.type || el.tagName).toLowerCase(),
      password:
        el.type === "password" ||
        (el.getAttribute("autocomplete") || "").toLowerCase().includes("password"),
    }));
  const links = Array.from(document.querySelectorAll("a[href]"))
    .slice(0, 50)
    .map((a) => ({ text: txt(a).slice(0, 120), href: a.href }))
    .filter((l) => l.text);
  return { url: location.href, title: document.title, text, fields, links, truncated };
}

/** The user's selected text. */
export function selectedText() {
  return String(window.getSelection() || "");
}

/** Clicks an element (by selector, or by its visible text or label); returns what it was called. */
export function clickTarget(selector, text) {
  const txt = (el) => (el.innerText ?? el.textContent ?? "").trim();
  const named = (el) =>
    (txt(el) || el.value || el.getAttribute("aria-label") || el.getAttribute("placeholder") || "")
      .trim()
      .toLowerCase();
  let el = null;
  if (selector) el = document.querySelector(selector);
  else if (text) {
    const wanted = text.trim().toLowerCase();
    const candidates = Array.from(
      document.querySelectorAll("a, button, input[type=submit], input[type=button], [role=button], [role=link]"),
    );
    el =
      candidates.find((c) => named(c) === wanted) ||
      candidates.find((c) => named(c).includes(wanted)) ||
      null;
  }
  if (!el) return { error: "notFound" };
  if (el.scrollIntoView) el.scrollIntoView({ block: "center" });
  el.click();
  return { clicked: (txt(el) || el.value || el.getAttribute("aria-label") || el.tagName).slice(0, 80) };
}

/** Types into a field (by selector, or by its label). Never into a password field. */
export function typeInto(selector, text, value, submit) {
  const txt = (el) => (el.innerText ?? el.textContent ?? "").trim();
  let el = null;
  if (selector) el = document.querySelector(selector);
  else if (text) {
    const wanted = text.trim().toLowerCase();
    const labels = Array.from(document.querySelectorAll("label"));
    const label =
      labels.find((l) => txt(l).toLowerCase() === wanted) ||
      labels.find((l) => txt(l).toLowerCase().includes(wanted));
    if (label) el = label.control || label.querySelector("input, textarea, select");
    if (!el) {
      el = Array.from(document.querySelectorAll("input, textarea, select")).find((f) =>
        [f.getAttribute("aria-label"), f.getAttribute("placeholder"), f.name]
          .filter(Boolean)
          .some((n) => n.toLowerCase().includes(wanted)),
      );
    }
  }
  if (!el) return { error: "notFound" };
  const autocomplete = (el.getAttribute("autocomplete") || "").toLowerCase();
  if (el.type === "password" || autocomplete.includes("password")) return { error: "password" };
  if (!("value" in el)) return { error: "notAField" };
  el.focus();
  const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), "value")?.set;
  if (setter) setter.call(el, value);
  else el.value = value;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
  if (submit) {
    if (el.form && typeof el.form.requestSubmit === "function") el.form.requestSubmit();
    else el.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
  }
  return { typed: true };
}
