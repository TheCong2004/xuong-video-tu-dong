/**
 * CapCut Automation palette — color by *role*, not rainbow noise.
 *
 * - Shell stays dark neutral
 * - Each domain (footage / audio / subtitle / nav / CTA) has one signature hue
 * - Avoid stacking every hue on the same control
 */
export const cc = {
  shell: "bg-[#0d1017]",
  nav: "bg-[#10141e]",
  panel: "bg-[#10141e]",
  surface: "bg-[#131926]",
  surfaceDeep: "bg-[#0b0f17]",

  /** Active nav / primary brand accent */
  brand: {
    text: "text-sky-300",
    soft: "bg-sky-500/15 text-sky-300",
    bar: "bg-sky-400",
    ring: "ring-1 ring-sky-400/35",
    border: "border-white/10",
  },

  /** Primary buttons (Apply / Generate / Export) */
  btnPrimary:
    "bg-[#2b7cff] text-white hover:bg-[#3a88ff] disabled:opacity-40",

  /** RUN FAB */
  btnRun:
    "bg-gradient-to-b from-cyan-300 to-cyan-500 text-[#0b1a1f] shadow-lg shadow-cyan-500/25 hover:brightness-110",

  /** Timeline track identities */
  track: {
    footage: {
      border: "border-white/10",
      bg: "bg-teal-900/45",
      header: "bg-teal-700/55",
    },
    audio: {
      border: "border-white/10",
      bg: "bg-blue-900/50",
      wave: "bg-sky-300/85",
    },
    subtitle: {
      border: "border-orange-400/35",
      bg: "bg-gradient-to-b from-orange-700/75 to-orange-950/65",
      text: "text-orange-50/95",
    },
  },

  /** Status chips */
  status: {
    synced: "border-white/10 bg-emerald-500/10 text-emerald-400",
    before: "bg-white/10 text-amber-400/90",
    beta: "bg-emerald-600 text-white",
    empty: "bg-white/10 text-white/40",
    active: "bg-sky-600 text-white",
  },

  /** Category library tabs (All / Video / Favorites) */
  tabActive: "bg-sky-500 text-white",
  tabIdle: "bg-[#2a2d35] text-white/55 hover:bg-[#32363f] hover:text-white/80",

  toggleOn: "bg-sky-400",
  toggleOff: "bg-white/15",
  accent: "accent-sky-500",
} as const;
