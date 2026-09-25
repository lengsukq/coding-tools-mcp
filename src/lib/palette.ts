/**
 * 调色盘系统
 *
 * 通过 <html data-palette="xxx"> 切换整套强调色。具体的 CSS 变量覆盖定义在
 * src/app.css 末尾的 "Palette system" 章节，本文件只负责调色盘元数据
 * （名称 / 预览色）与应用、持久化逻辑。
 *
 * 每个调色盘覆盖的 CSS 变量：
 * --primary / --primary-dark / --primary-soft / --accent-gradient(+hover)
 * --ios-blue / --accent-indigo / --accent-purple
 * --sidebar-active-text / --sidebar-active-border / --sidebar-surface-active
 * --surface-active / --focus-ring / --focus-border
 * --color-accent / --color-accent-hover / --card-border
 */

export interface PaletteThemeVars {
  /** 主色 */
  primary: string;
  /** 主色深色版（hover / active） */
  primaryDark: string;
  /** 主色浅色版（rgba，用于柔和背景） */
  primarySoft: string;
  /** 三段渐变 */
  gradient: string;
  gradientHover: string;
  /** 渐变中段 / 末段（供预览与点缀使用） */
  accentMid: string;
  accentEnd: string;
}

export interface PaletteDef {
  id: string;
  name: string;
  desc: string;
  /** 选择器里的预览三色点 */
  preview: [string, string, string];
  light: PaletteThemeVars;
  dark: PaletteThemeVars;
}

export const PALETTE_STORAGE_KEY = "coding-tools-palette";

export const PALETTES: PaletteDef[] = [
  {
    id: "ocean",
    name: "海蓝",
    desc: "经典默认，沉稳高效",
    preview: ["#0071e3", "#5856d6", "#af52de"],
    light: {
      primary: "#0071e3",
      primaryDark: "#0060c2",
      primarySoft: "rgba(0, 113, 227, 0.10)",
      gradient: "linear-gradient(135deg, #0071e3 0%, #5856d6 55%, #af52de 100%)",
      gradientHover: "linear-gradient(135deg, #0077ed 0%, #4d4bc2 55%, #9c40cb 100%)",
      accentMid: "#5856d6",
      accentEnd: "#af52de",
    },
    dark: {
      primary: "#0a84ff",
      primaryDark: "#0071e3",
      primarySoft: "rgba(10, 132, 255, 0.14)",
      gradient: "linear-gradient(135deg, #0a84ff 0%, #5e5ce6 55%, #bf5af2 100%)",
      gradientHover: "linear-gradient(135deg, #198fff 0%, #6e6cf4 55%, #c56cf0 100%)",
      accentMid: "#5e5ce6",
      accentEnd: "#bf5af2",
    },
  },
  {
    id: "coral",
    name: "珊瑚",
    desc: "温暖活力，热情洋溢",
    preview: ["#e2574c", "#e2845c", "#d64f9e"],
    light: {
      primary: "#e2574c",
      primaryDark: "#c74338",
      primarySoft: "rgba(226, 87, 76, 0.12)",
      gradient: "linear-gradient(135deg, #e2574c 0%, #e2845c 55%, #d64f9e 100%)",
      gradientHover: "linear-gradient(135deg, #ec655a 0%, #d9764f 55%, #c2448c 100%)",
      accentMid: "#e2845c",
      accentEnd: "#d64f9e",
    },
    dark: {
      primary: "#f27066",
      primaryDark: "#e2574c",
      primarySoft: "rgba(242, 112, 102, 0.14)",
      gradient: "linear-gradient(135deg, #f27066 0%, #f0925c 55%, #e05ca8 100%)",
      gradientHover: "linear-gradient(135deg, #f67f75 0%, #e5854f 55%, #cf4f97 100%)",
      accentMid: "#f0925c",
      accentEnd: "#e05ca8",
    },
  },
  {
    id: "grape",
    name: "葡萄紫",
    desc: "深邃优雅，富有创意",
    preview: ["#8e4ec6", "#6e56cf", "#d64f9e"],
    light: {
      primary: "#8e4ec6",
      primaryDark: "#7440a8",
      primarySoft: "rgba(142, 78, 198, 0.12)",
      gradient: "linear-gradient(135deg, #8e4ec6 0%, #6e56cf 55%, #d64f9e 100%)",
      gradientHover: "linear-gradient(135deg, #9a5cd0 0%, #6148bd 55%, #c2448c 100%)",
      accentMid: "#6e56cf",
      accentEnd: "#d64f9e",
    },
    dark: {
      primary: "#a87fe8",
      primaryDark: "#8e4ec6",
      primarySoft: "rgba(168, 127, 232, 0.14)",
      gradient: "linear-gradient(135deg, #a87fe8 0%, #7f6cf0 55%, #e05ca8 100%)",
      gradientHover: "linear-gradient(135deg, #b38ded 0%, #7260e0 55%, #cf4f97 100%)",
      accentMid: "#7f6cf0",
      accentEnd: "#e05ca8",
    },
  },
  {
    id: "emerald",
    name: "翡翠",
    desc: "清新自然，宁静专注",
    preview: ["#0d9d6c", "#0ea5a4", "#3b82f6"],
    light: {
      primary: "#0d9d6c",
      primaryDark: "#0a7d57",
      primarySoft: "rgba(13, 157, 108, 0.12)",
      gradient: "linear-gradient(135deg, #0d9d6c 0%, #0ea5a4 55%, #3b82f6 100%)",
      gradientHover: "linear-gradient(135deg, #0eac78 0%, #0d9494 55%, #3573e0 100%)",
      accentMid: "#0ea5a4",
      accentEnd: "#3b82f6",
    },
    dark: {
      primary: "#2fbf8f",
      primaryDark: "#0d9d6c",
      primarySoft: "rgba(47, 191, 143, 0.14)",
      gradient: "linear-gradient(135deg, #2fbf8f 0%, #22b8b0 55%, #5b9cf8 100%)",
      gradientHover: "linear-gradient(135deg, #3ccb9c 0%, #1fa8a1 55%, #4d8df0 100%)",
      accentMid: "#22b8b0",
      accentEnd: "#5b9cf8",
    },
  },
  {
    id: "amber",
    name: "琥珀",
    desc: "温暖醒目，充满能量",
    preview: ["#dd7d0a", "#e2574c", "#d64f9e"],
    light: {
      primary: "#dd7d0a",
      primaryDark: "#b56305",
      primarySoft: "rgba(221, 125, 10, 0.12)",
      gradient: "linear-gradient(135deg, #dd7d0a 0%, #e2574c 55%, #d64f9e 100%)",
      gradientHover: "linear-gradient(135deg, #ec8a12 0%, #d14a3f 55%, #c2448c 100%)",
      accentMid: "#e2574c",
      accentEnd: "#d64f9e",
    },
    dark: {
      primary: "#f5a623",
      primaryDark: "#dd7d0a",
      primarySoft: "rgba(245, 166, 35, 0.14)",
      gradient: "linear-gradient(135deg, #f5a623 0%, #f07f3c 55%, #e05ca8 100%)",
      gradientHover: "linear-gradient(135deg, #f7b13e 0%, #e07331 55%, #cf4f97 100%)",
      accentMid: "#f07f3c",
      accentEnd: "#e05ca8",
    },
  },
  {
    id: "sakura",
    name: "樱粉",
    desc: "柔美浪漫，轻盈灵动",
    preview: ["#d63384", "#af52de", "#7c5cf0"],
    light: {
      primary: "#d63384",
      primaryDark: "#b0256a",
      primarySoft: "rgba(214, 51, 132, 0.12)",
      gradient: "linear-gradient(135deg, #d63384 0%, #af52de 55%, #7c5cf0 100%)",
      gradientHover: "linear-gradient(135deg, #e04492 0%, #9c40cb 55%, #6d4fe0 100%)",
      accentMid: "#af52de",
      accentEnd: "#7c5cf0",
    },
    dark: {
      primary: "#f062b0",
      primaryDark: "#d63384",
      primarySoft: "rgba(240, 98, 176, 0.14)",
      gradient: "linear-gradient(135deg, #f062b0 0%, #c06cf0 55%, #8f7bf8 100%)",
      gradientHover: "linear-gradient(135deg, #f471b9 0%, #b25fe3 55%, #806df0 100%)",
      accentMid: "#c06cf0",
      accentEnd: "#8f7bf8",
    },
  },
];

export const DEFAULT_PALETTE_ID = "ocean";

export function getPalette(id: string | null | undefined): PaletteDef {
  return PALETTES.find((p) => p.id === id) ?? PALETTES[0];
}

export function getSavedPaletteId(): string {
  try {
    const saved = localStorage.getItem(PALETTE_STORAGE_KEY);
    if (saved && PALETTES.some((p) => p.id === saved)) return saved;
  } catch {
    /* localStorage 不可用时回退默认 */
  }
  return DEFAULT_PALETTE_ID;
}

/** 应用调色盘：写 data-palette 并持久化，返回生效的调色盘 */
export function applyPalette(id: string): PaletteDef {
  const palette = getPalette(id);
  document.documentElement.dataset.palette = palette.id;
  try {
    localStorage.setItem(PALETTE_STORAGE_KEY, palette.id);
  } catch {
    /* 忽略持久化失败 */
  }
  return palette;
}

/** 启动时调用：恢复上次选择的调色盘 */
export function initPalette(): PaletteDef {
  return applyPalette(getSavedPaletteId());
}
