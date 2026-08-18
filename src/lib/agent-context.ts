export const AGENT_SOURCE_OPTIONS = [
  { value: "auto", label: "自动扫描（推荐）", detail: "自动识别 Workspace 和常见全局 Instructions / Skills" },
  { value: "codex", label: "OpenAI Codex", detail: "AGENTS.md / .codex / .agents" },
  { value: "claude", label: "Claude Code", detail: "CLAUDE.md / .claude" },
  { value: "cursor", label: "Cursor", detail: ".cursor/rules / .cursor/skills" },
  { value: "copilot", label: "GitHub Copilot", detail: ".github instructions / skills" },
  { value: "opencode", label: "OpenCode", detail: "AGENTS.md / .opencode / .agents" },
  { value: "zcode", label: "ZCode", detail: "AGENTS.md / .zcode" },
  { value: "reasonix", label: "Reasonix", detail: "REASONIX.md / AGENTS.md / Skills" },
  { value: "custom", label: "Custom", detail: "自定义文件或 Skills 目录" },
] as const;

export type AgentSourceId = (typeof AGENT_SOURCE_OPTIONS)[number]["value"];

export function toggleSource(values: string[], value: string, enabled: boolean): string[] {
  if (value === "auto") {
    return enabled ? ["auto"] : ["disabled"];
  }
  const withoutAuto = values.filter((item) => item !== "auto" && item !== "disabled");
  if (enabled) return withoutAuto.includes(value) ? withoutAuto : [...withoutAuto, value];
  return values.filter((item) => item !== value);
}
