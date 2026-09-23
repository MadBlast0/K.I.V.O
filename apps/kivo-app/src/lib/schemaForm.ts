/**
 * Forms generated from a tool's JSON Schema (ROUT-09): the routine builder asks for a step's
 * arguments with one field per property. Strings, numbers, integers, booleans and enums get their
 * own inputs; anything else (a nested object, an array) is edited as JSON. A value may be a
 * phrase variable (`{minutes}`) wherever the schema wants a string or a number.
 */

export type FieldKind = "text" | "number" | "boolean" | "choice" | "json";

export interface FormField {
  name: string;
  kind: FieldKind;
  label: string;
  description?: string;
  required: boolean;
  choices?: string[];
}

type Schema = Record<string, unknown>;

function isObject(v: unknown): v is Schema {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function typeOf(schema: Schema): string | undefined {
  const type = schema["type"];
  if (typeof type === "string") return type;
  // `["string", "null"]`: an optional value.
  if (Array.isArray(type)) return type.find((x): x is string => typeof x === "string" && x !== "null");
  return undefined;
}

/** "file_name" → "File name". */
export function labelOf(name: string): string {
  const words = name
    .replace(/([a-z])([A-Z])/g, "$1 $2")
    .replace(/[_-]+/g, " ")
    .trim()
    .toLowerCase();
  return words.charAt(0).toUpperCase() + words.slice(1);
}

/** The input for a JSON Schema type. */
function kindOf(type: string | undefined): FieldKind {
  switch (type) {
    case "string":
      return "text";
    case "number":
    case "integer":
      return "number";
    case "boolean":
      return "boolean";
    default:
      return "json";
  }
}

/** The form's fields for a tool's `params` schema, required ones first. */
export function fieldsOf(schema: unknown): FormField[] {
  if (!isObject(schema) || !isObject(schema["properties"])) return [];
  const required = Array.isArray(schema["required"]) ? schema["required"].filter((r) => typeof r === "string") : [];
  const fields = Object.entries(schema["properties"]).map(([name, raw]): FormField => {
    const prop = isObject(raw) ? raw : {};
    const choices = Array.isArray(prop["enum"])
      ? prop["enum"].filter((x): x is string => typeof x === "string")
      : undefined;
    const choice = !!choices && choices.length > 0;
    return {
      name,
      kind: choice ? "choice" : kindOf(typeOf(prop)),
      label: typeof prop["title"] === "string" ? prop["title"] : labelOf(name),
      description: typeof prop["description"] === "string" ? prop["description"] : undefined,
      required: required.includes(name),
      choices: choice ? choices : undefined,
    };
  });
  return fields.toSorted((a, b) => Number(b.required) - Number(a.required));
}

/** A field's value as its input shows it. */
export function display(field: FormField, value: unknown): string {
  if (value === undefined || value === null) return "";
  if (typeof value === "string") return value;
  if (field.kind !== "json" && (typeof value === "number" || typeof value === "boolean")) return String(value);
  return JSON.stringify(value, null, 2);
}

/** Whether text is a phrase variable (`{minutes}`), which fills in when the routine runs. */
export const isVariable = (text: string) => /^\{[A-Za-z_][A-Za-z0-9_]*\}$/.test(text.trim());

/**
 * The argument value for what was typed: a number for a number field (or the variable as text),
 * parsed JSON for a JSON field; `undefined` removes the argument. Throws on JSON that doesn't
 * parse, with the parser's message.
 */
export function parseInput(field: FormField, text: string): unknown {
  const trimmed = text.trim();
  if (trimmed === "") return undefined;
  switch (field.kind) {
    case "number": {
      if (isVariable(trimmed)) return trimmed;
      const n = Number(trimmed);
      return Number.isFinite(n) ? n : trimmed;
    }
    case "json":
      return JSON.parse(trimmed) as unknown;
    default:
      return text;
  }
}

/** The required fields with no value. */
export function missing(fields: FormField[], args: Record<string, unknown>): string[] {
  return fields.filter((f) => f.required && (args[f.name] === undefined || args[f.name] === "")).map((f) => f.name);
}
