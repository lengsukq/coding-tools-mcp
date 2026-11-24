/**
 * Brand metadata and preference persistence.
 * Visual brand anchors live in palette.css; semantic surfaces, controls and state colors live in design-system.css.
 */

export interface PaletteDef {
  id: string;
  name: string;
  desc: string;
  /** 选择器里的预览三色点 */
  preview: [string, string, string];
}

const PALETTE_STORAGE_KEY = "coding-tools-palette";

export const PALETTES: PaletteDef[] = [
  {
    id: "ocean",
    name: "海蓝",
    desc: "经典默认，沉稳高效",
    preview: ["#0071e3", "#5856d6", "#af52de"],
  },
  {
    id: "coral",
    name: "珊瑚",
    desc: "温暖活力，热情洋溢",
    preview: ["#e2574c", "#e2845c", "#d64f9e"],
  },
  {
    id: "grape",
    name: "葡萄紫",
    desc: "深邃优雅，富有创意",
    preview: ["#8e4ec6", "#6e56cf", "#d64f9e"],
  },
  {
    id: "emerald",
    name: "翡翠",
    desc: "清新自然，宁静专注",
    preview: ["#0d9d6c", "#0ea5a4", "#3b82f6"],
  },
  {
    id: "amber",
    name: "琥珀",
    desc: "温暖醒目，充满能量",
    preview: ["#dd7d0a", "#e2574c", "#d64f9e"],
  },
  {
    id: "sakura",
    name: "樱粉",
    desc: "柔美浪漫，轻盈灵动",
    preview: ["#d63384", "#af52de", "#7c5cf0"],
  },
];

const DEFAULT_PALETTE_ID = "ocean";

function getPalette(id: string | null | undefined): PaletteDef {
  return PALETTES.find((p) => p.id === id) ?? PALETTES[0];
}

export function getSavedPaletteId(): string {
  try {
    const saved = localStorage.getItem(PALETTE_STORAGE_KEY);
    if (saved && PALETTES.some((p) => p.id === saved)) return saved;
  } catch (error) {
    console.warn("[palette] failed to read saved palette:", error);
  }
  return DEFAULT_PALETTE_ID;
}

/** 应用调色盘：写 data-palette 并持久化，返回生效的调色盘 */
export function applyPalette(id: string): PaletteDef {
  const palette = getPalette(id);
  document.documentElement.dataset.palette = palette.id;
  try {
    localStorage.setItem(PALETTE_STORAGE_KEY, palette.id);
  } catch (error) {
    console.warn("[palette] failed to persist palette:", error);
  }
  return palette;
}

/** 启动时调用：恢复上次选择的调色盘 */
export function initPalette(): PaletteDef {
  return applyPalette(getSavedPaletteId());
}
