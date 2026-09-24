/**
 * Capabilities → Computer use → Options (CAP-13, CAPABILITIES §4.1): what shows while KIVO
 * controls the screen (the frame, its cursor, the target highlight, the Island's controller), how
 * much it asks (every step, the first ten tasks, never; pausing when you use the mouse; speed), its
 * limits (steps, cost, time), and which apps it may touch (`apps` below, the shared allow and block
 * lists).
 */
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { bool, field, num, oneOf, setting, type Settings } from "../../lib/settings";
import { Group, Row, Section, Segmented, Select, Switch } from "../ui";

const STEPS = [10, 25, 50, 100] as const;
const CENTS = [10, 25, 50, 100, 200, 500] as const;
const SECONDS = [30, 60, 120, 300, 600, 1800] as const;

function pick<T extends string>(v: unknown, all: readonly T[], fallback: T): T {
  return oneOf(v, all) ?? fallback;
}

export function ComputerUseOptions({
  settings,
  save,
  apps,
}: {
  settings: Settings;
  save: (patch: Settings) => void;
  apps: ReactNode;
}) {
  const { t } = useTranslation();
  const cu = setting(settings, "tools", "computer-use");
  const set = (key: string, value: unknown) => save({ tools: { "computer-use": { [key]: value } } });
  const toggle = (key: string, fallback: boolean, title: string, hint?: string) => (
    <Row
      title={title}
      subtitle={hint}
      end={<Switch label={title} checked={bool(field(cu, key), fallback)} onChange={(v) => set(key, v)} />}
    />
  );
  const limit = (
    key: string,
    fallback: number,
    choices: readonly number[],
    show: (n: number) => string,
    label: string,
  ) => {
    const current = num(field(cu, key), fallback);
    const all = choices.includes(current) ? choices : [...choices, current].toSorted((a, b) => a - b);
    return (
      <Select<string>
        label={label}
        value={String(current)}
        onChange={(v) => set(key, Number(v))}
        items={all.map((n) => ({ value: String(n), label: show(n) }))}
      />
    );
  };
  return (
    <>
      <Section title={t("permissions.cu.visibility")} />
      <Group>
        <Row
          title={t("permissions.cu.frame")}
          subtitle={t("permissions.cu.frameHint")}
          end={
            <Segmented<"off" | "subtle" | "full">
              label={t("permissions.cu.frame")}
              value={pick(field(cu, "frame"), ["off", "subtle", "full"] as const, "subtle")}
              onChange={(v) => set("frame", v)}
              options={[
                { value: "off", label: t("permissions.cu.frames.off") },
                { value: "subtle", label: t("permissions.cu.frames.subtle") },
                { value: "full", label: t("permissions.cu.frames.full") },
              ]}
            />
          }
        />
        {toggle("show-cursor", true, t("permissions.cu.cursor"), t("permissions.cu.cursorHint"))}
        {toggle("highlight", true, t("permissions.cu.highlight"))}
        {toggle("island-controls", true, t("permissions.cu.island"), t("permissions.cu.islandHint"))}
      </Group>

      <Section title={t("permissions.cu.control")} />
      <Group>
        <Row
          title={t("permissions.cu.approve")}
          subtitle={t("permissions.cu.approveHint")}
          end={
            <Segmented<"always" | "first-tasks" | "never">
              label={t("permissions.cu.approve")}
              value={pick(field(cu, "approve"), ["always", "first-tasks", "never"] as const, "first-tasks")}
              onChange={(v) => set("approve", v)}
              options={[
                { value: "always", label: t("permissions.cu.approves.always") },
                { value: "first-tasks", label: t("permissions.cu.approves.firstTasks") },
                { value: "never", label: t("permissions.cu.approves.never") },
              ]}
            />
          }
        />
        {toggle("pause-on-mouse", true, t("permissions.cu.pauseOnMouse"), t("permissions.cu.pauseOnMouseHint"))}
        <Row
          title={t("permissions.cu.speed")}
          end={
            <Segmented<"careful" | "normal" | "fast">
              label={t("permissions.cu.speed")}
              value={pick(field(cu, "speed"), ["careful", "normal", "fast"] as const, "normal")}
              onChange={(v) => set("speed", v)}
              options={[
                { value: "careful", label: t("permissions.cu.speeds.careful") },
                { value: "normal", label: t("permissions.cu.speeds.normal") },
                { value: "fast", label: t("permissions.cu.speeds.fast") },
              ]}
            />
          }
        />
      </Group>

      <Section title={t("permissions.cu.limits")} />
      <Group>
        <Row
          title={t("permissions.cu.maxSteps")}
          end={limit(
            "max-steps",
            25,
            STEPS,
            (n) => t("permissions.cu.steps", { count: n }),
            t("permissions.cu.maxSteps"),
          )}
        />
        <Row
          title={t("permissions.cu.maxCost")}
          subtitle={t("permissions.cu.maxCostHint")}
          end={limit("max-cost-cents", 50, CENTS, (n) => `$${(n / 100).toFixed(2)}`, t("permissions.cu.maxCost"))}
        />
        <Row
          title={t("permissions.cu.timeLimit")}
          end={limit(
            "time-limit-seconds",
            120,
            SECONDS,
            (n) =>
              n < 60 ? t("permissions.cu.seconds", { count: n }) : t("permissions.cu.minutes", { count: n / 60 }),
            t("permissions.cu.timeLimit"),
          )}
        />
      </Group>

      <Section title={t("permissions.cu.apps")} />
      {apps}
    </>
  );
}
