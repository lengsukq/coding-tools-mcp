export const AGENT_SOURCE_OPTIONS = [
  { value: "auto", label: "自动扫描（推荐）", detail: "自动识别本机标准目录与 Workspace 内的 Instructions / Skills" },
  { value: "agents", label: "通用 Skills / Agents", detail: "~/.agents/skills / AGENTS.md" },
  { value: "codex", label: "OpenAI Codex", detail: "AGENTS.md / .codex / .agents" },
  { value: "claude", label: "Claude Code", detail: "CLAUDE.md / .claude" },
  { value: "cursor", label: "Cursor", detail: ".cursor/rules / .cursor/skills" },
  { value: "copilot", label: "GitHub Copilot", detail: ".github instructions / skills" },
  { value: "opencode", label: "OpenCode", detail: "AGENTS.md / .opencode / .agents" },
  { value: "zcode", label: "ZCode", detail: "AGENTS.md / .zcode / .agents" },
  { value: "reasonix", label: "Reasonix", detail: "REASONIX.md / AGENTS.md / Skills" },
  { value: "gemini", label: "Antigravity / Gemini", detail: "GEMINI.md / .gemini/skills" },
  { value: "custom", label: "Custom", detail: "自定义文件或 Skills 目录" },
] as const;

export function toggleSource(values: string[], value: string, enabled: boolean): string[] {
  if (value === "auto") {
    return enabled ? ["auto"] : ["disabled"];
  }
  const withoutAuto = values.filter((item) => item !== "auto" && item !== "disabled");
  if (enabled) return withoutAuto.includes(value) ? withoutAuto : [...withoutAuto, value];
  return values.filter((item) => item !== value);
}

export function isSourceRestricted(values: string[] | undefined): boolean {
  if (!values || values.length === 0) return false;
  return !values.includes("auto") && !values.includes("disabled");
}

export function toggleDisabledId(disabledIds: string[], id: string, enabled: boolean): string[] {
  if (enabled) {
    return disabledIds.filter((item) => item !== id);
  }
  return disabledIds.includes(id) ? disabledIds : [...disabledIds, id];
}

export function setBatchDisabledIds(
  disabledIds: string[],
  targetIds: string[],
  enabled: boolean,
): string[] {
  const targetSet = new Set(targetIds);
  if (enabled) {
    return disabledIds.filter((item) => !targetSet.has(item));
  }
  const merged = [...disabledIds];
  for (const id of targetIds) {
    if (!merged.includes(id)) merged.push(id);
  }
  return merged;
}
