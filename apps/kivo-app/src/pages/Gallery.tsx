/** Every component in one place, in both themes and all accents. The review surface for the design system. */
import type { TFunction } from "i18next";
import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { Island } from "../components/island/Island";
import {
  ISLAND_LIVE,
  ISLAND_NOTICES,
  ISLAND_STATES,
  islandPreset,
  islandStateLabel,
  type IslandState,
} from "../components/island/presets";
import { PageHeader } from "../components/layout/Shell";
import {
  EXPLORER_MENU,
  HelloPreview,
  JUMP_LIST,
  NativeMenuPreview,
  TRAY_MENU,
  TrayTooltipPreview,
  WindowsToastPreview,
} from "../components/system/SystemSurfaces";
import {
  AccentPicker,
  Alert,
  BudgetMeter,
  Button,
  Checkbox,
  ContextMenu,
  Dialog,
  DialogClose,
  Done,
  DropdownMenu,
  EmptyState,
  Group,
  IconButton,
  Keys,
  LevelMeter,
  Meta,
  Meter,
  Monogram,
  NewDot,
  Note,
  OptionCard,
  PageTabs,
  Pill,
  Popover,
  Radio,
  RadioGroup,
  Row,
  SearchField,
  Section,
  Segmented,
  Select,
  Sheet,
  ShortcutRecorder,
  Slider,
  Spinner,
  Stat,
  Switch,
  Tag,
  TextArea,
  TextField,
  Tile,
  Tooltip,
  useToast,
  type MenuEntry,
} from "../components/ui";
import { Icon, ICONS, isIconName } from "../icons";
import { useTheme, type MotionPref, type ThemePref } from "../lib/theme";

/** The demo menu, built per render so the labels follow the language. */
function menu(t: TFunction): MenuEntry[] {
  return [
    { type: "item", label: t("gallery.rename"), icon: "edit", shortcut: "F2" },
    { type: "item", label: t("gallery.duplicate"), icon: "layers", shortcut: "Ctrl+D" },
    {
      type: "submenu",
      label: t("gallery.moveTo"),
      icon: "folder",
      items: [
        { type: "item", label: t("gallery.work") },
        { type: "item", label: t("gallery.personal") },
        {
          type: "submenu",
          label: t("gallery.archive"),
          items: [
            { type: "item", label: "2026" },
            { type: "item", label: "2025" },
          ],
        },
      ],
    },
    { type: "separator" },
    { type: "label", label: t("gallery.run") },
    { type: "item", label: t("gallery.whenISignIn"), checked: true },
    { type: "item", label: t("gallery.everyMorning"), checked: false },
    { type: "separator" },
    { type: "item", label: t("gallery.delete"), icon: "delete", shortcut: "Del", danger: true },
  ];
}

export function Gallery() {
  const { t } = useTranslation();
  const { theme, setTheme, accent, setAccent, motion, setMotion } = useTheme();
  const toast = useToast();
  const [island, setIsland] = useState<IslandState>("listening");
  const [shortcut, setShortcut] = useState(["Ctrl", "Space"]);
  const [dialog, setDialog] = useState(false);
  const moreMenu = useMemo(() => menu(t), [t]);

  return (
    <>
      <PageHeader
        title={t("gallery.components")}
        subtitle={t("gallery.everyBuildingBlockOfKivo")}
        actions={
          <Segmented<ThemePref>
            label={t("gallery.theme")}
            value={theme}
            onChange={setTheme}
            options={[
              { value: "light", label: t("gallery.light") },
              { value: "dark", label: t("gallery.dark") },
              { value: "system", label: t("gallery.system") },
            ]}
          />
        }
      />

      <Section title={t("gallery.accent")} />
      <Group>
        <Row
          title={t("gallery.accentColour")}
          subtitle={t("gallery.usedForSelectionAndStatus")}
          end={<AccentPicker value={accent} onChange={setAccent} />}
        />
        <Row
          title={t("gallery.motion")}
          subtitle={t("gallery.reducedTurnsAnimationsOffIn")}
          end={
            <Segmented<MotionPref>
              label={t("gallery.motion")}
              value={motion}
              onChange={setMotion}
              options={[
                { value: "system", label: t("gallery.followWindows") },
                { value: "reduced", label: t("gallery.reduced") },
              ]}
            />
          }
        />
      </Group>

      <Section title={t("gallery.buttons")} />
      <Tile>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
          <Button variant="primary" icon="add">
            {t("gallery.add")}
          </Button>
          <Button>{t("gallery.secondary")}</Button>
          <Button variant="plain">{t("gallery.plain")}</Button>
          <Button variant="link">{t("gallery.link")}</Button>
          <Button variant="destructive" icon="delete">
            {t("gallery.delete")}
          </Button>
          <Button variant="stop" icon="stop">
            {t("gallery.stop")}
          </Button>
          <Button size="sm">{t("gallery.small")}</Button>
          <Button disabled>{t("gallery.disabled")}</Button>
          <Tooltip content={t("gallery.moreOptions")}>
            <IconButton icon="more" label={t("gallery.more")} />
          </Tooltip>
        </div>
      </Tile>

      <Section title={t("gallery.menus")} aside={t("gallery.dropdownWithSubmenusChecksShortcuts")} />
      <Tile>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
          <DropdownMenu trigger={<Button icon="more">{t("gallery.actions")}</Button>} items={moreMenu} />
          <Popover trigger={<Button icon="info">{t("gallery.popover")}</Button>} title={t("gallery.wakeWord")}>
            {t("gallery.sayHeyKivoFromAnywhere")}
          </Popover>
          <ContextMenu items={moreMenu}>
            <div
              style={{
                padding: "10px 14px",
                borderRadius: 8,
                border: "1px dashed var(--border)",
                color: "var(--text-2)",
              }}
            >
              {t("gallery.rightClickHere")}
            </div>
          </ContextMenu>
        </div>
      </Tile>

      <Section title={t("gallery.dialogsSheetsAndToasts")} />
      <Tile>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <Button onClick={() => setDialog(true)}>{t("gallery.dialog")}</Button>
          <Dialog
            open={dialog}
            onOpenChange={setDialog}
            title={t("gallery.forgetThisConversation")}
            description={t("gallery.kivoRemovesItFromMemory")}
            footer={
              <>
                <DialogClose>
                  <Button>{t("gallery.cancel")}</Button>
                </DialogClose>
                <DialogClose>
                  <Button variant="destructive">{t("gallery.forget")}</Button>
                </DialogClose>
              </>
            }
          />
          <Sheet trigger={<Button>{t("gallery.sideSheet")}</Button>} title={t("gallery.spotify")}>
            <Group>
              <Row
                lead={<Monogram text="S" color="#1DB954" />}
                title={t("gallery.spotify")}
                subtitle={t("gallery.connectedMusicPlayMusicSearch")}
              />
              <Row
                icon="permissions"
                title={t("gallery.permissions")}
                subtitle={t("gallery.askBeforePurchases")}
                onClick={() => toast(t("gallery.opensSpotifySPermissions"))}
              />
            </Group>
          </Sheet>
          <Button onClick={() => toast(t("gallery.saved"))}>{t("gallery.toast")}</Button>
          <Button
            onClick={() => toast(t("gallery.moved3FilesToArchive"), { onUndo: () => toast(t("gallery.restored")) })}
          >
            {t("gallery.toastWithUndo")}
          </Button>
        </div>
      </Tile>

      <Section title={t("gallery.tabs")} />
      <PageTabs
        label={t("gallery.extensions")}
        tabs={[
          {
            value: "connectors",
            label: t("gallery.connectors"),
            content: (
              <Group>
                <Row
                  icon="connector"
                  title={t("gallery.gmail")}
                  subtitle={t("gallery.connected")}
                  end={<Tag tone="success">{t("gallery.on")}</Tag>}
                />
              </Group>
            ),
          },
          {
            value: "mcp",
            label: t("gallery.mcpServers"),
            content: (
              <Group>
                <Row
                  icon="server"
                  title={t("gallery.filesystem")}
                  subtitle={t("gallery.local12Tools")}
                  end={<Switch defaultChecked label={t("gallery.enableFilesystem")} />}
                />
              </Group>
            ),
          },
          {
            value: "plugins",
            label: t("gallery.plugins"),
            content: (
              <EmptyState
                icon="plugin"
                title={t("gallery.noPluginsYet")}
                action={
                  <Button variant="primary" icon="add">
                    {t("gallery.browsePlugins")}
                  </Button>
                }
              >
                {t("gallery.pluginsBundleConnectorsSkillsAnd")}
              </EmptyState>
            ),
          },
          {
            value: "skills",
            label: t("gallery.skills"),
            content: (
              <Group>
                <Row
                  icon="skill"
                  title={t("gallery.summarizePage")}
                  subtitle={t("gallery.foundOnThisPc")}
                  end={<NewDot />}
                />
              </Group>
            ),
          },
        ]}
      />

      <Section title={t("gallery.controls")} />
      <Group>
        <Row
          icon="mic"
          title={t("gallery.wakeWord")}
          subtitle={t("gallery.listenForHeyKivo")}
          end={<Switch defaultChecked label={t("gallery.wakeWord")} />}
        />
        <Row
          icon="speed"
          title={t("gallery.speechRate")}
          end={
            <div style={{ width: 160 }}>
              <Slider label={t("gallery.speechRate")} defaultValue={60} />
            </div>
          }
        />
        <Row
          icon="theme"
          title={t("gallery.theme")}
          end={
            <Select
              label={t("gallery.theme")}
              value={theme}
              onChange={setTheme}
              items={[
                { value: "light", label: t("gallery.light") },
                { value: "dark", label: t("gallery.dark") },
                { value: "system", label: t("gallery.system") },
              ]}
            />
          }
        />
        <Row
          icon="keyboard"
          title={t("gallery.pushToTalk")}
          end={
            <ShortcutRecorder
              value={shortcut}
              onChange={setShortcut}
              conflict={(k) => (k.join("+") === "Ctrl+C" ? t("gallery.usedForCopy", { keys: "Ctrl+C" }) : null)}
            />
          }
        />
        <Row
          icon="volume"
          title={t("gallery.sounds")}
          end={
            <Segmented
              label={t("gallery.sounds")}
              defaultValue="subtle"
              options={[
                { value: "off", label: t("gallery.off") },
                { value: "subtle", label: t("gallery.subtle") },
                { value: "full", label: t("gallery.full") },
              ]}
            />
          }
        />
      </Group>
      <div style={{ height: 12 }} />
      <Tile>
        <div style={{ display: "grid", gap: 10 }}>
          <Checkbox defaultChecked>{t("gallery.showTranscriptWhileISpeak")}</Checkbox>
          <RadioGroup label={t("gallery.position")} defaultValue="top">
            <Radio value="top">{t("gallery.topOfScreen")}</Radio>
            <Radio value="bottom">{t("gallery.bottomOfScreen")}</Radio>
          </RadioGroup>
          <TextField icon="globe" placeholder="https://" />
          <SearchField placeholder={t("gallery.searchSettings")} />
          <TextArea placeholder={t("gallery.notesForKivo")} rows={3} />
        </div>
      </Tile>
      <div style={{ height: 12 }} />
      <RadioGroup label={t("gallery.permissionMode")} defaultValue="auto">
        <div style={{ display: "grid", gap: 8 }}>
          <OptionCard
            value="ask"
            title={t("gallery.askEveryTime")}
            description={t("gallery.kivoAsksBeforeAnyChange")}
          />
          <OptionCard
            value="auto"
            title={t("gallery.auto")}
            description={t("gallery.routineActionsRunAutomaticallyOnly")}
            badge={<Pill tone="accent">{t("gallery.recommended")}</Pill>}
          />
          <OptionCard
            value="bypass"
            title={t("gallery.bypassPermissions")}
            description={t("gallery.everythingRunsWithoutAskingTurns")}
            badge={<Pill tone="danger">{t("gallery.useWithCare")}</Pill>}
          />
        </div>
      </RadioGroup>

      <Section title={t("gallery.status")} />
      <Tile>
        <div style={{ display: "flex", gap: 10, flexWrap: "wrap", alignItems: "center" }}>
          <Tag tone="success">{t("gallery.connected")}</Tag>
          <Tag tone="warning">{t("gallery.needsSignIn")}</Tag>
          <Tag tone="danger">{t("gallery.error")}</Tag>
          <Tag>{t("gallery.off")}</Tag>
          <Pill tone="accent">{t("gallery.new")}</Pill>
          <Pill>{t("gallery.local")}</Pill>
          <Keys keys={["Ctrl", "Alt", "Shift", "Esc"]} />
          <NewDot />
          <Spinner />
          <Done />
          <Monogram text="GH" color="#24292F" />
        </div>
        <div style={{ display: "grid", gap: 12, marginTop: 14 }}>
          <Meter value={42} label={t("gallery.contextUsed")} />
          <BudgetMeter value={83} label={t("gallery.monthlySpend")} />
          <LevelMeter />
        </div>
      </Tile>
      <div
        style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))", gap: 12, marginTop: 12 }}
      >
        <Stat value={t("gallery.statSeconds", { value: 1.4 })} label={t("gallery.medianResponse")} />
        <Stat value={t("gallery.statMoney", { value: 3.2 })} label={t("gallery.thisMonth")} />
        <Stat value={t("gallery.statCount", { value: 128 })} label={t("gallery.actionsToday")} />
      </div>

      <Section title={t("gallery.alertsAndEmptyStates")} />
      <div style={{ display: "grid", gap: 8 }}>
        <Alert kind="info" title={t("gallery.updatesAreCheckedDaily")}>
          {t("gallery.youCanChangeThisIn")}
        </Alert>
        <Alert kind="success" title={t("gallery.voiceProfileTrained")} />
        <Alert kind="warning" title={t("gallery.bypassIsOn")}>
          {t("gallery.itTurnsOffIn52", { minutes: 52 })}
        </Alert>
        <Alert kind="danger" title={t("gallery.microphoneUnavailable")}>
          {t("gallery.anotherAppIsUsingIt")}
        </Alert>
      </div>
      <Note>{t("gallery.notesExplainASettingIn")}</Note>
      <Meta>{t("gallery.updated2MinutesAgo")}</Meta>

      <Section title={t("gallery.island")} aside={t("gallery.statesAndLiveActivities")} />
      <div className="k-gallery-stage">
        <Island model={islandPreset(island, { partial: t("gallery.partial") })} />
      </div>
      <div className="k-gallery-chips">
        {[...ISLAND_STATES, ...ISLAND_NOTICES, ...ISLAND_LIVE].map((id) => (
          <button
            key={id}
            type="button"
            className="k-gallery-chip"
            aria-pressed={island === id}
            onClick={() => setIsland(id)}
          >
            {islandStateLabel(id)}
          </button>
        ))}
      </div>

      <Section title={t("gallery.systemSurfaces")} aside={t("gallery.previewsOfNativeWindowsUi")} />
      <div className="k-gallery-surfaces">
        <figure>
          <NativeMenuPreview entries={TRAY_MENU} label={t("gallery.trayMenu")} />
          <figcaption>{t("gallery.trayMenuRightClickThe")}</figcaption>
        </figure>
        <figure>
          <TrayTooltipPreview state={t("gallery.listening")} mode={t("gallery.auto")} tasks={1} />
          <figcaption>{t("gallery.trayTooltipHoverTheTray")}</figcaption>
        </figure>
        <figure>
          <NativeMenuPreview entries={JUMP_LIST} label={t("gallery.jumpList")} width={230} />
          <figcaption>{t("gallery.jumpListRightClickKivo")}</figcaption>
        </figure>
        <figure>
          <WindowsToastPreview
            title={t("gallery.kivoIsStillRunning")}
            text={t("gallery.sayHeyKivoOrPress")}
            actions={[t("gallery.settings"), t("palette.quit")]}
          />
          <figcaption>{t("gallery.firstClose")}</figcaption>
        </figure>
        <figure>
          <WindowsToastPreview title={t("gallery.claudeFinishedInKI")} text={t("gallery.all48TestsPassWant")} reply />
          <figcaption>{t("gallery.taskFinishedReplyInline")}</figcaption>
        </figure>
        <figure>
          <WindowsToastPreview
            title={t("gallery.n80OfYourMonthlyBudget")}
            text={t("gallery.n802Of1000")}
            actions={[t("gallery.openUsage"), t("gallery.ok")]}
          />
          <figcaption>{t("gallery.budgetWarning")}</figcaption>
        </figure>
        <figure>
          <WindowsToastPreview
            title={t("gallery.kivo02IsReady")}
            text={t("gallery.takesAbout20SecondsAnything")}
            actions={[t("gallery.installNow"), t("gallery.whenIdle")]}
          />
          <figcaption>{t("gallery.updateReady")}</figcaption>
        </figure>
        <figure>
          <WindowsToastPreview
            title={t("gallery.kivoCanTUseThe")}
            text={t("gallery.windowsIsBlockingMicrophoneAccess")}
            actions={[t("gallery.openWindowsSettings"), t("gallery.typeInstead")]}
          />
          <figcaption>{t("gallery.microphoneBlocked")}</figcaption>
        </figure>
        <figure>
          <HelloPreview title={t("gallery.kivoWantsToSendAn")} detail={t("gallery.toMayaStudioComDesign")} />
          <figcaption>{t("gallery.windowsHelloHighRiskActions")}</figcaption>
        </figure>
        <figure>
          <NativeMenuPreview entries={EXPLORER_MENU} label={t("gallery.fileExplorerMenu")} width={230} />
          <figcaption>{t("gallery.fileExplorerMenuRightClick")}</figcaption>
        </figure>
      </div>

      <Section title={t("gallery.icons")} aside={t("gallery.iconCount", { count: Object.keys(ICONS).length })} />
      <div className="k-gallery-icons">
        {Object.keys(ICONS)
          .filter(isIconName)
          .map((n) => (
            <div key={n} title={n}>
              <Icon name={n} size={18} />
              <span>{n}</span>
            </div>
          ))}
      </div>
    </>
  );
}
