/**
 * Settings → Shortcuts (UX-31): keys that work everywhere. Select one to change it; the runtime
 * binds the new keys at once. A chord another KIVO shortcut already uses isn't taken. Ctrl+K
 * (inside the Control Center) and the emergency stop are fixed: the stop always works, even when
 * another app uses the same keys (SECURITY §8).
 */
import { useTranslation } from "react-i18next";
import { Group, Keys, Row, ShortcutRecorder, Switch } from "../../components/ui";
import { bool, strings } from "../../lib/settings";
import { useConfig } from "./useConfig";

interface Changeable {
  id: string;
  group: string;
  key: string;
  icon: "mic" | "chat" | "permissions" | "pause";
  optional: boolean;
}

const CHANGEABLE: ReadonlyArray<Changeable> = [
  { id: "talk", group: "voice", key: "push-to-talk", icon: "mic", optional: false },
  { id: "type", group: "voice", key: "type-to-kivo", icon: "chat", optional: false },
  { id: "mode", group: "permissions", key: "mode-shortcut", icon: "permissions", optional: true },
  { id: "pause", group: "voice", key: "pause-shortcut", icon: "pause", optional: true },
];

const same = (a: string[], b: string[]) =>
  a.length === b.length && a.every((k) => b.some((x) => x.toLowerCase() === k.toLowerCase()));

export function ShortcutsTab() {
  const { t } = useTranslation();
  const { get, set } = useConfig();
  const stop = strings(get("permissions", "emergency-stop"));
  const chords = CHANGEABLE.map((c) => ({
    id: c.id,
    group: c.group,
    key: c.key,
    icon: c.icon,
    optional: c.optional,
    keys: strings(get(c.group, c.key)),
  }));
  /** The shortcut that already uses `keys`, other than `id`. */
  const usedBy = (id: string, keys: string[]) => {
    const other = chords.find((c) => c.id !== id && c.keys.length > 0 && same(c.keys, keys));
    if (other) return t(`settings.shortcuts.${other.id}`);
    if (same(keys, stop)) return t("settings.shortcuts.stop");
    if (same(keys, ["Ctrl", "K"])) return t("settings.shortcuts.search");
    return null;
  };

  const row = (c: (typeof chords)[number]) => (
    <Row
      key={c.id}
      icon={c.icon}
      title={t(`settings.shortcuts.${c.id}`)}
      end={
        <ShortcutRecorder
          value={c.keys}
          conflict={(keys) => {
            const other = usedBy(c.id, keys);
            return other ? t("settings.shortcuts.taken", { name: other }) : null;
          }}
          onChange={(keys) => {
            if (!usedBy(c.id, keys)) set(c.group, { [c.key]: keys });
          }}
          onClear={c.optional ? () => set(c.group, { [c.key]: [] }) : undefined}
        />
      }
    />
  );

  return (
    <Group>
      {row(chords[0])}
      <Row
        icon="hand"
        title={t("settings.shortcuts.toggle")}
        subtitle={t("settings.shortcuts.toggleHint")}
        end={
          <Switch
            label={t("settings.shortcuts.toggle")}
            checked={bool(get("voice", "toggle-mode"))}
            onChange={(v) => set("voice", { "toggle-mode": v })}
          />
        }
      />
      <Row
        icon="wave"
        title={t("settings.shortcuts.autoEnd")}
        subtitle={t("settings.shortcuts.autoEndHint")}
        end={
          <Switch
            label={t("settings.shortcuts.autoEnd")}
            checked={bool(get("voice", "auto-end-on-silence"), true)}
            onChange={(v) => set("voice", { "auto-end-on-silence": v })}
          />
        }
      />
      {row(chords[1])}
      <Row
        icon="search"
        title={t("settings.shortcuts.search")}
        subtitle={t("settings.shortcuts.searchHint")}
        end={<Keys keys={["Ctrl", "K"]} />}
      />
      {row(chords[2])}
      {row(chords[3])}
      <Row
        icon="stop"
        title={t("settings.shortcuts.stop")}
        subtitle={t("settings.shortcuts.stopHint")}
        end={<Keys keys={stop} />}
      />
    </Group>
  );
}
