/** Product modes (plan §142, PLAN-06): one switch for how KIVO behaves right now. The runtime maps
 * each onto the privacy mode, the performance profile and the Island (`mode.set`); Normal puts the
 * user's own settings back. */
import type { IconName } from "../icons";
import type { ProductMode } from "../ipc/generated";

export const MODES: { mode: ProductMode; icon: IconName }[] = [
  { mode: "normal", icon: "home" },
  { mode: "private", icon: "privacy" },
  { mode: "offline", icon: "cloud" },
  { mode: "battery", icon: "power" },
  { mode: "performance", icon: "speed" },
  { mode: "gaming", icon: "monitor" },
  { mode: "presentation", icon: "frame" },
];
