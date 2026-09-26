/**
 * "Microphone and speaker" (UX §4, the first setup step): which microphone KIVO listens with and
 * which speaker or headphones it talks through, each following the system's default until the
 * user picks another, and a test for each: say something and watch the bars, play a sound.
 */
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Method } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { Button, Group, Note, Row, Section, Select, useToast } from "../ui";
import { MicCheck } from "../voice/MicCheck";

interface Device {
  id: string;
  name: string;
  isDefault: boolean;
}

const DEFAULT = "default";
const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

export function DevicesStep() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [devices, setDevices] = useState<{ inputs: Device[]; outputs: Device[] } | null>(null);
  const [chosen, setChosen] = useState<{ input: string; output: string; sounds: string } | null>(null);
  const [played, setPlayed] = useState(false);
  useEffect(() => {
    if (!connected) return;
    void request<{ inputs: Device[]; outputs: Device[] }>(Method.voiceDevices)
      .then(setDevices)
      .catch(() => setDevices({ inputs: [], outputs: [] }));
    void request<{ voice: Record<string, unknown>; sounds: Record<string, unknown> }>(Method.settingsGet)
      .then((s) => {
        const id = (v: unknown) => (typeof v === "string" && v ? v : DEFAULT);
        setChosen({
          input: id(s.voice["input-device"]),
          output: id(s.voice["output-device"]),
          sounds: typeof s.sounds["set"] === "string" ? s.sounds["set"] : "soft",
        });
      })
      .catch(() => {});
  }, [connected, request]);
  if (!devices || !chosen) return null;

  // "System default (Realtek Audio)": the default follows the system when it changes.
  const items = (list: Device[]) => {
    const current = list.find((d) => d.isDefault)?.name;
    return [
      {
        value: DEFAULT,
        label: current
          ? t("onboarding.devices.systemDefaultNamed", { name: current })
          : t("onboarding.devices.systemDefault"),
      },
      ...list.map((d) => ({ value: d.id, label: d.name })),
    ];
  };
  const pick = (key: "input-device" | "output-device", value: string) => {
    request(Method.settingsSet, { voice: { [key]: value === DEFAULT ? null : value } })
      .then(() => setChosen((c) => c && (key === "input-device" ? { ...c, input: value } : { ...c, output: value })))
      .catch((e: unknown) => toast(message(e)));
  };
  const play = () => {
    request(Method.soundsPreview, { set: chosen.sounds })
      .then(() => setPlayed(true))
      .catch((e: unknown) => toast(message(e)));
  };

  return (
    <>
      <Section title={t("onboarding.devices.microphone")} />
      <MicCheck>
        <Row
          icon="mic"
          title={t("onboarding.devices.listensWith")}
          end={
            <Select
              label={t("onboarding.devices.microphone")}
              value={chosen.input}
              onChange={(v) => pick("input-device", v)}
              items={items(devices.inputs)}
            />
          }
        />
      </MicCheck>
      <Section title={t("onboarding.devices.speaker")} />
      <Group>
        <Row
          icon="volume"
          title={t("onboarding.devices.talksThrough")}
          end={
            <Select
              label={t("onboarding.devices.speaker")}
              value={chosen.output}
              onChange={(v) => pick("output-device", v)}
              items={items(devices.outputs)}
            />
          }
        />
        <Row
          icon="play"
          title={t("onboarding.devices.testSpeaker")}
          subtitle={t("onboarding.devices.testSpeakerHint")}
          end={
            <Button size="sm" icon="play" onClick={play}>
              {played ? t("onboarding.devices.playAgain") : t("onboarding.devices.play")}
            </Button>
          }
        />
      </Group>
      {played && <Note>{t("onboarding.devices.notHeard")}</Note>}
    </>
  );
}
