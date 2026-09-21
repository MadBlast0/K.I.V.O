/**
 * Semantic icon set. Rule (DESIGN_SYSTEM.md → Icon rules): one meaning per icon and one icon
 * per meaning. Components use the semantic name, never a Lucide name directly, so an icon can
 * be swapped in one place.
 */
import type { LucideProps } from "lucide-react";
import type { ComponentType } from "react";
import {
  Accessibility,
  Activity,
  AlertTriangle,
  AppWindow,
  AudioLines,
  BarChart3,
  Bell,
  BookOpen,
  Bot,
  Brain,
  Check,
  ChevronDown,
  ChevronRight,
  Clipboard,
  Clock,
  Cloud,
  Code,
  Coins,
  Compass,
  Contrast,
  Cpu,
  Database,
  Download,
  Ellipsis,
  ExternalLink,
  Eye,
  EyeOff,
  FileText,
  Folder,
  Frame,
  Gauge,
  Globe,
  GripVertical,
  Hand,
  Headphones,
  HelpCircle,
  Home,
  Inbox,
  Info,
  Keyboard,
  KeyRound,
  Layers,
  Lightbulb,
  Link,
  List,
  ListChecks,
  Lock,
  Maximize2,
  Merge,
  MessageSquare,
  Mic,
  Minimize2,
  Monitor,
  Moon,
  Mouse,
  MousePointer2,
  Music,
  Palette,
  Pause,
  Pencil,
  Play,
  Plug,
  Plus,
  Power,
  Puzzle,
  RefreshCw,
  Repeat,
  Repeat2,
  Rows3,
  Search,
  Send,
  Server,
  Settings,
  Shield,
  SlidersHorizontal,
  Smartphone,
  Sparkles,
  Square,
  SquareTerminal,
  Stethoscope,
  Sun,
  Trash2,
  Type,
  Undo2,
  Upload,
  User,
  Users,
  Volume2,
  Wind,
  X,
  Zap,
} from "lucide-react";

/** Custom glyph: the Island capsule. */
const IslandGlyph = (props: LucideProps) => (
  <svg
    viewBox="0 0 24 24"
    width={props.size ?? 16}
    height={props.size ?? 16}
    fill="none"
    stroke="currentColor"
    strokeWidth={props.strokeWidth ?? 1.6}
    strokeLinecap="round"
    strokeLinejoin="round"
    className={props.className}
    aria-hidden
  >
    <path d="M7 9h10a3 3 0 0 1 0 6H7a3 3 0 0 1 0-6z" />
    <path d="M9 12h.01" />
  </svg>
);

export const ICONS = {
  // navigation
  home: Home,
  chat: MessageSquare,
  tasks: ListChecks,
  activity: Activity,
  routine: Repeat2,
  brain: Brain,
  agent: Bot,
  voice: Mic,
  extensions: Plug,
  permissions: Shield,
  memory: Database,
  usage: BarChart3,
  settings: Settings,
  // concepts
  mic: Mic,
  connector: Plug,
  server: Server,
  plugin: Zap,
  skill: Puzzle,
  capabilities: SlidersHorizontal,
  privacy: Lock,
  ai: Sparkles,
  performance: Gauge,
  diagnostics: Stethoscope,
  info: Info,
  accessibility: Accessibility,
  // actions
  search: Search,
  add: Plus,
  play: Play,
  stop: Square,
  pause: Pause,
  check: Check,
  close: X,
  edit: Pencil,
  delete: Trash2,
  refresh: RefreshCw,
  undo: Undo2,
  send: Send,
  external: ExternalLink,
  more: Ellipsis,
  download: Download,
  upload: Upload,
  link: Link,
  grip: GripVertical,
  chevronRight: ChevronRight,
  chevronDown: ChevronDown,
  // things
  warning: AlertTriangle,
  eye: Eye,
  eyeOff: EyeOff,
  hand: Hand,
  terminal: SquareTerminal,
  clipboard: Clipboard,
  folder: Folder,
  globe: Globe,
  cpu: Cpu,
  cloud: Cloud,
  clock: Clock,
  music: Music,
  bell: Bell,
  user: User,
  users: Users,
  keyboard: Keyboard,
  wave: AudioLines,
  code: Code,
  power: Power,
  window: AppWindow,
  help: HelpCircle,
  list: List,
  cursor: MousePointer2,
  idea: Lightbulb,
  theme: Contrast,
  glow: Sun,
  headset: Headphones,
  palette: Palette,
  file: FileText,
  layers: Layers,
  tray: Inbox,
  compass: Compass,
  frame: Frame,
  repeat: Repeat,
  merge: Merge,
  motion: Wind,
  compress: Minimize2,
  book: BookOpen,
  rows: Rows3,
  resize: Maximize2,
  volume: Volume2,
  type: Type,
  key: KeyRound,
  coin: Coins,
  mouse: Mouse,
  phone: Smartphone,
  moon: Moon,
  monitor: Monitor,
  speed: Zap,
  island: IslandGlyph,
} satisfies Record<string, ComponentType<LucideProps>>;

export type IconName = keyof typeof ICONS;

/** Narrows a string to a known icon name without an unchecked cast. */
export const isIconName = (name: string): name is IconName => Object.hasOwn(ICONS, name);

export function Icon({
  name,
  size = 16,
  className,
  strokeWidth = 1.6,
}: {
  name: IconName;
  size?: number;
  className?: string;
  strokeWidth?: number;
}) {
  const C = ICONS[name];
  return <C size={size} strokeWidth={strokeWidth} className={className} aria-hidden />;
}
