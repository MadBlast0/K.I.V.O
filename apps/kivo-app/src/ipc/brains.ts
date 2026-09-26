/**
 * The shapes of the runtime's brain requests (BRAINS §4–5, §9; CONVERSATION §0–1), as the
 * runtime sends them (`apps/kivo-runtime/src/brains_rpc.rs`). Keys never appear here: the
 * runtime only says whether one is stored (SEC-19).
 */

export type ProviderKind = "api" | "cli" | "local" | "managedLogin";
export type PrivacyClass = "local" | "cloud";
export type SignIn = "cliLogin" | "oAuth" | "local" | "apiKey";

export type Health =
  { state: "ready" } | { state: "needsSignIn" } | { state: "rateLimited" } | { state: "unreachable"; reason: string };

export interface CatalogEntry {
  id: string;
  name: string;
  kind: ProviderKind;
  privacy: PrivacyClass;
  signIn: SignIn;
  free: string | null;
  baseUrl: string;
  names: string[];
}

export interface CliTool {
  id: string;
  commands: string[];
  adapters: string[];
  install: string;
  adapterInstall: string;
}

export interface Catalog {
  brains: CatalogEntry[];
  cli: CliTool[];
}

export interface BrainView {
  id: string;
  name: string;
  kind: ProviderKind;
  privacy: PrivacyClass;
  free: string | null;
  enabled: boolean;
  health: Health;
  hasKey: boolean;
  models: string[];
  checkedAgo: number | null;
}

export type ProfilePrivacy = "cloud" | "localPreferred" | "strictPrivate";
export type Tier = "default" | "fast" | "smart" | "cheap" | "coding";

/** How hard a model thinks before it answers (`kivo_brain::reasoning::Effort`). */
export type Effort = "off" | "low" | "medium" | "high";

export interface ModelRef {
  provider: string;
  model: string;
  /** The user's reasoning level; absent is the model's own default. */
  reasoning?: Effort;
}

export interface Profile {
  id: string;
  name: string;
  tier: Tier;
  primary: ModelRef | null;
  fallbacks: ModelRef[];
  privacy: ProfilePrivacy;
  maxContext: number | null;
  allowedTools: { kind: "all" } | { kind: "none" } | { kind: "only"; tools: string[] };
  systemPromptAddendum: string;
  persona: string | null;
  /** Voice requests on this profile open a realtime conversation (BRAIN-33). */
  realtime?: boolean;
  builtIn: boolean;
}

export interface BrainsList {
  connected: BrainView[];
  profiles: Profile[];
  defaultProfile: string;
  /** The brain KIVO uses (the default profile's choice); null is Automatic. */
  active: ModelRef | null;
  persona: string;
  customPersona: string;
  cliAgentsOn: boolean;
  cloudOn: boolean;
  workspace: string;
}

export interface DiscoveredItem {
  id: string;
  data: {
    name?: string;
    cli?: string | null;
    program?: string | null;
    version?: string | null;
    signedIn?: boolean | null;
    needsAdapter?: boolean;
    adapterInstall?: string;
    free?: string | null;
    url?: string;
    models?: string[];
  };
  new: boolean;
}

export interface DiscoverySection {
  section: string;
  items: DiscoveredItem[];
  checkedAt: number | null;
}

export interface Conversation {
  id: string;
  kind: "voice" | "chat";
  title: string;
  summary: string;
  summarized: number;
  brain: string | null;
  pinned: boolean;
  createdAt: number;
  updatedAt: number;
}

export interface StoredMessage {
  id: number;
  conversationId: string;
  ts: number;
  role: "user" | "assistant" | "tool";
  text: string;
  brain: string | null;
  turnId: string | null;
}

export interface ContextLayer {
  id: "system" | "instructions" | "workspace" | "live" | "skills" | "memories" | "summary" | "turns" | "tools";
  text?: string;
  tokens: number;
  max?: number;
  count?: number;
  /** Turned off in Settings → Context (CONV-31). */
  on?: boolean;
}

export interface ContextPreview {
  brain: string | null;
  budget: { voice: number; chat: number };
  /** A new conversation's first message, in dollars; null when the price isn't known. */
  sessionCost?: number | null;
  /** The whole request the next message would send, secrets redacted. */
  preview?: string | null;
  previewTokens?: number | null;
  layers: ContextLayer[];
}

export function healthTone(h: Health): "success" | "warning" | "danger" | "neutral" {
  switch (h.state) {
    case "ready":
      return "success";
    case "rateLimited":
      return "warning";
    case "needsSignIn":
      return "warning";
    case "unreachable":
      return "danger";
  }
}

/** Two letters for a brain's monogram. */
export function monogram(name: string): string {
  const words = name.split(/\s+/).filter(Boolean);
  const letters = words.length > 1 ? words[0][0] + words[1][0] : name.slice(0, 2);
  return letters.toUpperCase();
}

/** A steady colour per brain, for its monogram. */
export function brainColor(id: string): string {
  const known: Record<string, string> = {
    "claude-code": "#C96442",
    anthropic: "#C96442",
    "gemini-cli": "#4285F4",
    gemini: "#4285F4",
    codex: "#10A37F",
    openai: "#10A37F",
    openrouter: "#4B4BD6",
    ollama: "#1D1D1F",
    lmstudio: "#6B4BD6",
    llamacpp: "#444444",
    groq: "#F55036",
    mistral: "#FA520F",
    deepseek: "#4D6BFE",
    xai: "#111111",
    opencode: "#1D1D1F",
  };
  return known[id] ?? "#6E6E73";
}
