// Types for page.js (used by the Control Center's tests of the extension).

export interface Field {
  selector: string;
  label: string;
  kind: string;
  password: boolean;
}

export interface Page {
  url: string;
  title: string;
  text: string;
  fields: Field[];
  links: { text: string; href: string }[];
  truncated: boolean;
}

export function readPage(maxChars: number): Page;
export function selectedText(): string;
export function clickTarget(
  selector: string | null,
  text: string | null,
): { clicked: string } | { error: string };
export function typeInto(
  selector: string | null,
  text: string | null,
  value: string,
  submit: boolean,
): { typed: true } | { error: string };
