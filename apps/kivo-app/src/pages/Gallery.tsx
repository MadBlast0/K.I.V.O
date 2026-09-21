/** Every component in one place, in both themes and all accents. The review surface for the design system. */
import { useState } from "react";
import { Island } from "../components/island/Island";
import {
  ISLAND_LIVE,
  ISLAND_NOTICES,
  ISLAND_STATES,
  islandPreset,
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

const MORE_MENU: MenuEntry[] = [
  { type: "item", label: "Rename", icon: "edit", shortcut: "F2" },
  { type: "item", label: "Duplicate", icon: "layers", shortcut: "Ctrl+D" },
  {
    type: "submenu",
    label: "Move to",
    icon: "folder",
    items: [
      { type: "item", label: "Work" },
      { type: "item", label: "Personal" },
      {
        type: "submenu",
        label: "Archive",
        items: [
          { type: "item", label: "2026" },
          { type: "item", label: "2025" },
        ],
      },
    ],
  },
  { type: "separator" },
  { type: "label", label: "Run" },
  { type: "item", label: "When I sign in", checked: true },
  { type: "item", label: "Every morning", checked: false },
  { type: "separator" },
  { type: "item", label: "Delete", icon: "delete", shortcut: "Del", danger: true },
];

export function Gallery() {
  const { theme, setTheme, accent, setAccent, motion, setMotion } = useTheme();
  const toast = useToast();
  const [island, setIsland] = useState<IslandState>("listening");
  const [shortcut, setShortcut] = useState(["Ctrl", "Space"]);
  const [dialog, setDialog] = useState(false);

  return (
    <>
      <PageHeader
        title="Components"
        subtitle="Every building block of KIVO. Switch theme and accent to check both."
        actions={
          <Segmented<ThemePref>
            label="Theme"
            value={theme}
            onChange={setTheme}
            options={[
              { value: "light", label: "Light" },
              { value: "dark", label: "Dark" },
              { value: "system", label: "System" },
            ]}
          />
        }
      />

      <Section title="Accent" />
      <Group>
        <Row
          title="Accent colour"
          subtitle="Used for selection and status only"
          end={<AccentPicker value={accent} onChange={setAccent} />}
        />
        <Row
          title="Motion"
          subtitle="Reduced turns animations off in KIVO"
          end={
            <Segmented<MotionPref>
              label="Motion"
              value={motion}
              onChange={setMotion}
              options={[
                { value: "system", label: "Follow Windows" },
                { value: "reduced", label: "Reduced" },
              ]}
            />
          }
        />
      </Group>

      <Section title="Buttons" />
      <Tile>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
          <Button variant="primary" icon="add">
            Add
          </Button>
          <Button>Secondary</Button>
          <Button variant="plain">Plain</Button>
          <Button variant="link">Link</Button>
          <Button variant="destructive" icon="delete">
            Delete
          </Button>
          <Button variant="stop" icon="stop">
            Stop
          </Button>
          <Button size="sm">Small</Button>
          <Button disabled>Disabled</Button>
          <Tooltip content="More options">
            <IconButton icon="more" label="More" />
          </Tooltip>
        </div>
      </Tile>

      <Section title="Menus" aside="Dropdown with submenus, checks, shortcuts; right-click menu" />
      <Tile>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap", alignItems: "center" }}>
          <DropdownMenu trigger={<Button icon="more">Actions</Button>} items={MORE_MENU} />
          <Popover trigger={<Button icon="info">Popover</Button>} title="Wake word">
            Say “Hey Kivo” from anywhere. KIVO listens only for the wake word until you say it.
          </Popover>
          <ContextMenu items={MORE_MENU}>
            <div
              style={{
                padding: "10px 14px",
                borderRadius: 8,
                border: "1px dashed var(--border)",
                color: "var(--text-2)",
              }}
            >
              Right-click here
            </div>
          </ContextMenu>
        </div>
      </Tile>

      <Section title="Dialogs, sheets and toasts" />
      <Tile>
        <div style={{ display: "flex", gap: 8, flexWrap: "wrap" }}>
          <Button onClick={() => setDialog(true)}>Dialog</Button>
          <Dialog
            open={dialog}
            onOpenChange={setDialog}
            title="Forget this conversation?"
            description="KIVO removes it from memory and history. You can’t undo this."
            footer={
              <>
                <DialogClose>
                  <Button>Cancel</Button>
                </DialogClose>
                <DialogClose>
                  <Button variant="destructive">Forget</Button>
                </DialogClose>
              </>
            }
          />
          <Sheet trigger={<Button>Side sheet</Button>} title="Spotify">
            <Group>
              <Row
                lead={<Monogram text="S" color="#1DB954" />}
                title="Spotify"
                subtitle="Connected · music.play, music.search"
              />
              <Row
                icon="permissions"
                title="Permissions"
                subtitle="Ask before purchases"
                onClick={() => toast("Opens Spotify’s permissions")}
              />
            </Group>
          </Sheet>
          <Button onClick={() => toast("Saved")}>Toast</Button>
          <Button onClick={() => toast("Moved 3 files to Archive", { onUndo: () => toast("Restored") })}>
            Toast with Undo
          </Button>
        </div>
      </Tile>

      <Section title="Tabs" />
      <PageTabs
        label="Extensions"
        tabs={[
          {
            value: "connectors",
            label: "Connectors",
            content: (
              <Group>
                <Row icon="connector" title="Gmail" subtitle="Connected" end={<Tag tone="success">On</Tag>} />
              </Group>
            ),
          },
          {
            value: "mcp",
            label: "MCP servers",
            content: (
              <Group>
                <Row
                  icon="server"
                  title="filesystem"
                  subtitle="Local · 12 tools"
                  end={<Switch defaultChecked label="Enable filesystem" />}
                />
              </Group>
            ),
          },
          {
            value: "plugins",
            label: "Plugins",
            content: (
              <EmptyState
                icon="plugin"
                title="No plugins yet"
                action={
                  <Button variant="primary" icon="add">
                    Browse plugins
                  </Button>
                }
              >
                Plugins bundle connectors, skills and routines.
              </EmptyState>
            ),
          },
          {
            value: "skills",
            label: "Skills",
            content: (
              <Group>
                <Row icon="skill" title="Summarize page" subtitle="Found on this PC" end={<NewDot />} />
              </Group>
            ),
          },
        ]}
      />

      <Section title="Controls" />
      <Group>
        <Row
          icon="mic"
          title="Wake word"
          subtitle="Listen for “Hey Kivo”"
          end={<Switch defaultChecked label="Wake word" />}
        />
        <Row
          icon="speed"
          title="Speech rate"
          end={
            <div style={{ width: 160 }}>
              <Slider label="Speech rate" defaultValue={60} />
            </div>
          }
        />
        <Row
          icon="theme"
          title="Theme"
          end={
            <Select
              label="Theme"
              value={theme}
              onChange={setTheme}
              items={[
                { value: "light", label: "Light" },
                { value: "dark", label: "Dark" },
                { value: "system", label: "System" },
              ]}
            />
          }
        />
        <Row
          icon="keyboard"
          title="Push to talk"
          end={
            <ShortcutRecorder
              value={shortcut}
              onChange={setShortcut}
              conflict={(k) => (k.join("+") === "Ctrl+C" ? "Ctrl+C is used for Copy" : null)}
            />
          }
        />
        <Row
          icon="volume"
          title="Sounds"
          end={
            <Segmented
              label="Sounds"
              defaultValue="subtle"
              options={[
                { value: "off", label: "Off" },
                { value: "subtle", label: "Subtle" },
                { value: "full", label: "Full" },
              ]}
            />
          }
        />
      </Group>
      <div style={{ height: 12 }} />
      <Tile>
        <div style={{ display: "grid", gap: 10 }}>
          <Checkbox defaultChecked>Show transcript while I speak</Checkbox>
          <RadioGroup label="Position" defaultValue="top">
            <Radio value="top">Top of screen</Radio>
            <Radio value="bottom">Bottom of screen</Radio>
          </RadioGroup>
          <TextField icon="globe" placeholder="https://" />
          <SearchField placeholder="Search settings" />
          <TextArea placeholder="Notes for KIVO…" rows={3} />
        </div>
      </Tile>
      <div style={{ height: 12 }} />
      <RadioGroup label="Permission mode" defaultValue="auto">
        <div style={{ display: "grid", gap: 8 }}>
          <OptionCard value="ask" title="Ask every time" description="KIVO asks before any change." />
          <OptionCard
            value="auto"
            title="Auto"
            description="Routine actions run automatically. Only high-risk actions ask."
            badge={<Pill tone="accent">Recommended</Pill>}
          />
          <OptionCard
            value="bypass"
            title="Bypass permissions"
            description="Everything runs without asking. Turns off after 1 hour."
            badge={<Pill tone="danger">Use with care</Pill>}
          />
        </div>
      </RadioGroup>

      <Section title="Status" />
      <Tile>
        <div style={{ display: "flex", gap: 10, flexWrap: "wrap", alignItems: "center" }}>
          <Tag tone="success">Connected</Tag>
          <Tag tone="warning">Needs sign-in</Tag>
          <Tag tone="danger">Error</Tag>
          <Tag>Off</Tag>
          <Pill tone="accent">New</Pill>
          <Pill>Local</Pill>
          <Keys keys={["Ctrl", "Alt", "Shift", "Esc"]} />
          <NewDot />
          <Spinner />
          <Done />
          <Monogram text="GH" color="#24292F" />
        </div>
        <div style={{ display: "grid", gap: 12, marginTop: 14 }}>
          <Meter value={42} label="Context used" />
          <BudgetMeter value={83} label="Monthly spend" />
          <LevelMeter />
        </div>
      </Tile>
      <div
        style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(140px, 1fr))", gap: 12, marginTop: 12 }}
      >
        <Stat value="1.4 s" label="Median response" />
        <Stat value="$3.20" label="This month" />
        <Stat value="128" label="Actions today" />
      </div>

      <Section title="Alerts and empty states" />
      <div style={{ display: "grid", gap: 8 }}>
        <Alert kind="info" title="Updates are checked daily.">
          You can change this in Settings.
        </Alert>
        <Alert kind="success" title="Voice profile trained." />
        <Alert kind="warning" title="Bypass is on.">
          It turns off in 52 minutes.
        </Alert>
        <Alert kind="danger" title="Microphone unavailable.">
          Another app is using it.
        </Alert>
      </div>
      <Note>Notes explain a setting in one or two lines, under its group.</Note>
      <Meta>Updated 2 minutes ago</Meta>

      <Section title="Island" aside="States and live activities" />
      <div className="k-gallery-stage">
        <Island model={islandPreset(island, { partial: "Play something calm on Spotify and" })} />
      </div>
      <div className="k-gallery-chips">
        {[...ISLAND_STATES, ...ISLAND_NOTICES, ...ISLAND_LIVE].map((s) => (
          <button
            key={s.id}
            type="button"
            className="k-gallery-chip"
            aria-pressed={island === s.id}
            onClick={() => setIsland(s.id)}
          >
            {s.label}
          </button>
        ))}
      </div>

      <Section title="System surfaces" aside="Previews of native Windows UI" />
      <div className="k-gallery-surfaces">
        <figure>
          <NativeMenuPreview entries={TRAY_MENU} label="Tray menu" />
          <figcaption>Tray menu · right-click the tray icon</figcaption>
        </figure>
        <figure>
          <TrayTooltipPreview state="Listening" mode="Auto" tasks={1} />
          <figcaption>Tray tooltip · hover the tray icon</figcaption>
        </figure>
        <figure>
          <NativeMenuPreview entries={JUMP_LIST} label="Jump list" width={230} />
          <figcaption>Jump list · right-click KIVO in the taskbar</figcaption>
        </figure>
        <figure>
          <WindowsToastPreview
            title="KIVO is still running"
            text="Say “Hey Kivo” or press Ctrl + Space anytime. Quit from the tray icon."
            actions={["Settings", "Quit KIVO"]}
          />
          <figcaption>First close</figcaption>
        </figure>
        <figure>
          <WindowsToastPreview title="Claude finished in K.I.V.O" text="All 48 tests pass. Want me to commit?" reply />
          <figcaption>Task finished · reply inline</figcaption>
        </figure>
        <figure>
          <WindowsToastPreview
            title="80% of your monthly budget used"
            text="$8.02 of $10.00. At the limit, KIVO will ask before using cloud AI."
            actions={["Open Usage", "OK"]}
          />
          <figcaption>Budget warning</figcaption>
        </figure>
        <figure>
          <WindowsToastPreview
            title="KIVO 0.2 is ready"
            text="Takes about 20 seconds. Anything running is saved first."
            actions={["Install now", "When idle"]}
          />
          <figcaption>Update ready</figcaption>
        </figure>
        <figure>
          <WindowsToastPreview
            title="KIVO can’t use the microphone"
            text="Windows is blocking microphone access for desktop apps."
            actions={["Open Windows settings", "Type instead"]}
          />
          <figcaption>Microphone blocked</figcaption>
        </figure>
        <figure>
          <HelloPreview
            title="KIVO wants to send an email"
            detail="To maya@studio.com: “Design review moved to Thursday”"
          />
          <figcaption>Windows Hello · high-risk actions</figcaption>
        </figure>
        <figure>
          <NativeMenuPreview entries={EXPLORER_MENU} label="File Explorer menu" width={230} />
          <figcaption>File Explorer menu · right-click a file</figcaption>
        </figure>
      </div>

      <Section title="Icons" aside={`${Object.keys(ICONS).length} semantic names`} />
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
