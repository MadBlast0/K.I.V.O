/**
 * Every setting the Ctrl+K palette can find (UX-47): where it lives (a Settings tab or another
 * page) and the translation key of its name. Kept beside the tabs so a new setting is listed where
 * it's added.
 */
import type { IconName } from "../../icons";
import type { PageId } from "../../components/layout/Shell";
import type { SettingsTab } from "../Settings";

export interface SettingEntry {
  /** A Settings tab, or another page. */
  tab?: SettingsTab;
  page?: PageId;
  label: string;
  icon: IconName;
  /** More words it's found by. */
  keywords?: string;
}

export const SETTINGS_INDEX: ReadonlyArray<SettingEntry> = [
  { tab: "general", label: "settings.general.startWithWindows", icon: "power", keywords: "startup autostart login" },
  { tab: "general", label: "settings.general.keepRunning", icon: "tray", keywords: "close tray background" },
  { tab: "general", label: "settings.general.trayIcon", icon: "tray", keywords: "notification area" },
  { tab: "general", label: "settings.general.lowMemory", icon: "cpu", keywords: "ram light slow" },
  { tab: "general", label: "settings.general.spoken", icon: "mic", keywords: "language speak" },
  { tab: "general", label: "settings.general.format", icon: "clock", keywords: "date time number region" },
  { tab: "general", label: "settings.general.export", icon: "upload", keywords: "backup save file" },
  { tab: "general", label: "settings.general.import", icon: "download", keywords: "restore file" },
  { tab: "general", label: "settings.general.reset", icon: "refresh", keywords: "defaults" },
  { tab: "appearance", label: "settings.appearance.theme", icon: "theme", keywords: "dark light mode" },
  { tab: "appearance", label: "settings.appearance.accent", icon: "palette", keywords: "colour color" },
  { tab: "appearance", label: "settings.appearance.textSize", icon: "type", keywords: "font bigger" },
  { tab: "appearance", label: "settings.appearance.motion", icon: "motion", keywords: "animation reduce" },
  { tab: "appearance", label: "settings.appearance.transparency", icon: "layers", keywords: "mica frosted" },
  { tab: "island", label: "settings.island.style", icon: "island", keywords: "hidden orb pill" },
  { tab: "island", label: "settings.island.position", icon: "island", keywords: "top bottom drag" },
  { tab: "island", label: "settings.island.size", icon: "resize", keywords: "compact large" },
  { tab: "island", label: "settings.island.monitor", icon: "monitor", keywords: "screen display" },
  { tab: "island", label: "settings.island.transcript", icon: "chat", keywords: "words live" },
  { tab: "island", label: "settings.island.hideAfter", icon: "clock", keywords: "collapse seconds" },
  { tab: "island", label: "settings.island.fullscreen", icon: "window", keywords: "games" },
  { tab: "sounds", label: "sounds.enabled", icon: "volume", keywords: "earcon cues" },
  { tab: "sounds", label: "sounds.set", icon: "music", keywords: "soft glass pulse wood" },
  { tab: "sounds", label: "sounds.volume", icon: "speed" },
  { tab: "notifications", label: "settings.notify.speak", icon: "wave", keywords: "out loud" },
  { tab: "notifications", label: "settings.notify.toasts", icon: "bell", keywords: "windows notifications toast" },
  { tab: "notifications", label: "settings.notify.quietHours", icon: "moon", keywords: "night do not disturb" },
  { tab: "notifications", label: "settings.notify.focus", icon: "bell", keywords: "focus" },
  {
    tab: "accessibility",
    label: "settings.access.announcements",
    icon: "accessibility",
    keywords: "narrator screen reader",
  },
  { tab: "accessibility", label: "settings.access.captions", icon: "chat" },
  { tab: "accessibility", label: "settings.access.voice-only", icon: "wave", keywords: "blind" },
  { tab: "shortcuts", label: "settings.shortcuts.talk", icon: "mic", keywords: "push to talk hotkey key" },
  { tab: "shortcuts", label: "settings.shortcuts.toggle", icon: "hand", keywords: "toggle hold press" },
  { tab: "shortcuts", label: "settings.shortcuts.autoEnd", icon: "wave", keywords: "silence pause stop" },
  { tab: "shortcuts", label: "settings.shortcuts.type", icon: "chat", keywords: "hotkey keyboard" },
  { tab: "shortcuts", label: "settings.shortcuts.pause", icon: "pause", keywords: "hotkey" },
  { tab: "performance", label: "settings.performance.profile", icon: "speed", keywords: "battery gaming" },
  { tab: "diagnostics", label: "settings.diagnostics.run", icon: "diagnostics", keywords: "check problem" },
  { tab: "about", label: "settings.about.licenses", icon: "file", keywords: "version open source" },
  { page: "voice", label: "palette.settings.wake", icon: "mic", keywords: "hey kivo wake word" },
  { page: "voice", label: "palette.settings.voice", icon: "wave", keywords: "tts speech speaking" },
  { page: "permissions", label: "palette.settings.mode", icon: "permissions", keywords: "ask auto plan bypass" },
  { page: "permissions", label: "palette.settings.privacy", icon: "privacy", keywords: "cloud local private" },
  { page: "brains", label: "palette.settings.context", icon: "layers", keywords: "context compaction preview tokens" },
  { page: "brains", label: "palette.settings.brains", icon: "brain", keywords: "ai model provider" },
  { page: "memory", label: "palette.settings.memory", icon: "memory", keywords: "remember notes obsidian" },
  { page: "usage", label: "palette.settings.budget", icon: "coin", keywords: "cost limit money" },
];
