<script lang="ts">
  import { scanAgentContext, type AgentContextSnapshotDto } from "$lib/api/agent-context";
  import { AGENT_SOURCE_OPTIONS, toggleSource } from "$lib/agent-context";

  export interface RuntimePolicyDraft {
    toolProfile: string;
    permissionMode: string;
    allowedCommands: string;
    executablePaths: string;
    aiInstructions: string;
    instructionSources: string[];
    skillSources: string[];
    customInstructionPaths: string;
    customSkillPaths: string;
    workspaceLocalEntries: boolean;
    workspaceScriptExtensions: string;
  }

  interface Props {
    workspaceId: string;
    toolProfile: string;
    permissionMode: string;
    allowedCommands: string;
    executablePaths: string;
    aiInstructions: string;
    instructionSources: string[];
    skillSources: string[];
    customInstructionPaths: string;
    customSkillPaths: string;
    workspaceLocalEntries: boolean;
    workspaceScriptExtensions: string;
    onSave: (draft: RuntimePolicyDraft) => void | Promise<void>;
  }

  const TOOL_PROFILE_OPTIONS = [
    { value: "full", label: "完整工具" },
    { value: "read-only", label: "只读工具" },
    { value: "compat-readonly-all", label: "兼容只读" },
  ] as const;

  const PERMISSION_MODE_OPTIONS = [
    { value: "trusted", label: "受信任" },
    { value: "safe", label: "安全受限" },
    { value: "dangerous", label: "完全放开" },
  ] as const;

  let {
    workspaceId,
    toolProfile,
    permissionMode,
    allowedCommands,
    executablePaths,
    aiInstructions,
    instructionSources,
    skillSources,
    customInstructionPaths,
    customSkillPaths,
    workspaceLocalEntries,
    workspaceScriptExtensions,
    onSave,
  }: Props = $props();

  let draftProfile = $state("full");
  let draftMode = $state("trusted");
  let draftCommands = $state("");
  let draftExecutablePaths = $state("");
  let draftAiInstructions = $state("");
  let draftInstructionSources = $state<string[]>([]);
  let draftSkillSources = $state<string[]>([]);
  let draftCustomInstructionPaths = $state("");
  let draftCustomSkillPaths = $state("");
  let draftLocalEntries = $state(true);
  let draftExtensions = $state(".exe,.bat,.cmd,.ps1");
  let saving = $state(false);
  let scanning = $state(false);
  let scanResult = $state<AgentContextSnapshotDto | null>(null);
  let scanError = $state("");

  function arraysEqual(a: string[], b: string[]) {
    return a.length === b.length && a.every((value, index) => value === b[index]);
  }

  const dirty = $derived(
    draftProfile !== toolProfile
      || draftMode !== permissionMode
      || draftCommands !== allowedCommands
      || draftExecutablePaths !== executablePaths
      || draftAiInstructions !== aiInstructions
      || !arraysEqual(draftInstructionSources, instructionSources)
      || !arraysEqual(draftSkillSources, skillSources)
      || draftCustomInstructionPaths !== customInstructionPaths
      || draftCustomSkillPaths !== customSkillPaths
      || draftLocalEntries !== workspaceLocalEntries
      || draftExtensions !== workspaceScriptExtensions,
  );

  $effect(() => {
    draftProfile = toolProfile;
    draftMode = permissionMode;
    draftCommands = allowedCommands;
    draftExecutablePaths = executablePaths;
    draftAiInstructions = aiInstructions;
    draftInstructionSources = [...instructionSources];
    draftSkillSources = [...skillSources];
    draftCustomInstructionPaths = customInstructionPaths;
    draftCustomSkillPaths = customSkillPaths;
    draftLocalEntries = workspaceLocalEntries;
    draftExtensions = workspaceScriptExtensions;
  });

  async function save() {
    if (saving || !dirty) return;
    saving = true;
    try {
      await onSave({
        toolProfile: draftProfile,
        permissionMode: draftMode,
        allowedCommands: draftCommands.trim(),
        executablePaths: draftExecutablePaths.trim(),
        aiInstructions: draftAiInstructions.trim(),
        instructionSources: [...draftInstructionSources],
        skillSources: [...draftSkillSources],
        customInstructionPaths: draftCustomInstructionPaths.trim(),
        customSkillPaths: draftCustomSkillPaths.trim(),
        workspaceLocalEntries: draftLocalEntries,
        workspaceScriptExtensions: draftExtensions.trim(),
      });
      scanResult = null;
    } finally {
      saving = false;
    }
  }

  async function scan() {
    if (scanning) return;
    scanning = true;
    scanError = "";
    try {
      if (dirty) await save();
      scanResult = await scanAgentContext(workspaceId);
    } catch (error) {
      scanError = String(error);
    } finally {
      scanning = false;
    }
  }
</script>

<form
  class="grid gap-4"
  onsubmit={(event) => {
    event.preventDefault();
    void save();
  }}
>
  <label class="grid gap-1">
    <span class="text-xs text-[var(--color-text-muted)]">工具档位</span>
    <select
      class="rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-2.5 py-1.5 text-sm"
      bind:value={draftProfile}
    >
      {#each TOOL_PROFILE_OPTIONS as option}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
  </label>

  <label class="grid gap-1">
    <span class="text-xs text-[var(--color-text-muted)]">系统命令（逗号分隔）</span>
    <input type="text" class="tx-input tx-mono" placeholder="python,git,gh,aws,cargo,..." bind:value={draftCommands} />
  </label>

  <label class="grid gap-1">
    <span class="text-xs text-[var(--color-text-muted)]">额外可执行 PATH（每行一个目录，也可粘贴 PATH）</span>
    <textarea class="min-h-20 rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-2.5 py-1.5 font-mono text-sm" placeholder="/opt/homebrew/bin&#10;/usr/local/bin&#10;~/.cargo/bin" bind:value={draftExecutablePaths}></textarea>
    <span class="text-xs text-[var(--color-text-muted)]">Workspace PATH 优先于 Global PATH，再回退 System PATH；命令仍需加入白名单。</span>
  </label>

  <div class="rounded-md border border-[var(--color-border)] p-3">
    <div class="mb-3">
      <p class="text-sm font-medium">Agent Context Sources</p>
      <p class="mt-1 text-xs text-[var(--color-text-muted)]">
        Workspace 未选择来源时继承全局设置；如果全局也未选择，则自动识别常见 Agent 配置。Instructions 会在 MCP 初始化时重扫，Skills 会在 list/get 时实时重扫。
      </p>
    </div>

    <div class="grid gap-3 md:grid-cols-2">
      <div>
        <p class="mb-2 text-xs font-medium">Instructions</p>
        <div class="grid gap-2">
          {#each AGENT_SOURCE_OPTIONS as option}
            <label class="flex items-start gap-2 text-sm">
              <input
                type="checkbox"
                checked={draftInstructionSources.includes(option.value)}
                onchange={(event) => {
                  draftInstructionSources = toggleSource(
                    draftInstructionSources,
                    option.value,
                    event.currentTarget.checked,
                  );
                }}
              />
              <span>
                <span class="block">{option.label}</span>
                <span class="block text-[11px] text-[var(--color-text-muted)]">{option.detail}</span>
              </span>
            </label>
          {/each}
        </div>
      </div>

      <div>
        <p class="mb-2 text-xs font-medium">Skills</p>
        <div class="grid gap-2">
          {#each AGENT_SOURCE_OPTIONS as option}
            <label class="flex items-start gap-2 text-sm">
              <input
                type="checkbox"
                checked={draftSkillSources.includes(option.value)}
                onchange={(event) => {
                  draftSkillSources = toggleSource(
                    draftSkillSources,
                    option.value,
                    event.currentTarget.checked,
                  );
                }}
              />
              <span>
                <span class="block">{option.label}</span>
                <span class="block text-[11px] text-[var(--color-text-muted)]">{option.detail}</span>
              </span>
            </label>
          {/each}
        </div>
      </div>
    </div>

    {#if draftInstructionSources.includes("custom") || draftSkillSources.includes("custom")}
      <div class="mt-4 grid gap-3 md:grid-cols-2">
        {#if draftInstructionSources.includes("custom")}
          <label class="grid gap-1">
            <span class="text-xs text-[var(--color-text-muted)]">自定义 Instructions 文件（每行一个）</span>
            <textarea class="min-h-20 tx-input tx-mono" placeholder="docs/AI_RULES.md" bind:value={draftCustomInstructionPaths}></textarea>
          </label>
        {/if}
        {#if draftSkillSources.includes("custom")}
          <label class="grid gap-1">
            <span class="text-xs text-[var(--color-text-muted)]">自定义 Skills 根目录（每行一个）</span>
            <textarea class="min-h-20 tx-input tx-mono" placeholder=".my-agent/skills" bind:value={draftCustomSkillPaths}></textarea>
          </label>
        {/if}
      </div>
    {/if}

    <div class="mt-4 flex flex-wrap items-center gap-2">
      <button type="button" class="tx-btn-ghost" disabled={scanning} onclick={() => void scan()}>
        {scanning ? "扫描中…" : "立即重新扫描"}
      </button>
      {#if scanResult}
        <span class="text-xs text-[var(--color-text-muted)]">
          已发现 {scanResult.instructions.length} 个 Instructions · {scanResult.skills.length} 个 Skills
        </span>
      {/if}
    </div>
    {#if scanError}<p class="mt-2 text-xs text-[var(--danger)]">{scanError}</p>{/if}
    {#if scanResult && (scanResult.instructions.length > 0 || scanResult.skills.length > 0)}
      <details class="mt-3 text-xs">
        <summary class="cursor-pointer text-[var(--color-text-muted)]">查看发现结果</summary>
        <div class="mt-2 grid gap-2">
          {#each scanResult.instructions as item}
            <div><strong>[{item.provider}]</strong> {item.path}</div>
          {/each}
          {#each scanResult.skills as skill}
            <div><strong>Skill · {skill.name}</strong> · {skill.provider} · {skill.path}</div>
          {/each}
        </div>
      </details>
    {/if}
  </div>

  <label class="grid gap-1">
    <span class="text-xs text-[var(--color-text-muted)]">Workspace AI Instructions</span>
    <textarea class="min-h-28 rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-2.5 py-1.5 text-sm" placeholder="例如：修改代码必须遵循 Clean Code；运行测试后再确认完成。" bind:value={draftAiInstructions}></textarea>
    <span class="text-xs text-[var(--color-text-muted)]">手动规则会与 Global Instructions、自动发现的 Repository Instructions 一起注入。</span>
  </label>

  <label class="flex items-center gap-2 text-sm">
    <input type="checkbox" bind:checked={draftLocalEntries} />
    <span>允许执行 Workspace 内本地入口</span>
  </label>
  <label class="grid gap-1">
    <span class="text-xs text-[var(--color-text-muted)]">本地脚本扩展名（逗号分隔）</span>
    <input type="text" class="tx-input tx-mono" placeholder=".exe,.bat,.cmd,.ps1" bind:value={draftExtensions} disabled={!draftLocalEntries} />
  </label>
  <label class="grid gap-1">
    <span class="text-xs text-[var(--color-text-muted)]">权限模式</span>
    <select class="rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-2.5 py-1.5 text-sm" bind:value={draftMode}>
      {#each PERMISSION_MODE_OPTIONS as option}
        <option value={option.value}>{option.label}</option>
      {/each}
    </select>
  </label>
  <p class="text-xs text-[var(--color-text-muted)]">
    P0 仅自动注入 repository-wide / always-apply 规则；按文件 glob、生效目录等动态 scoped rules 后续由 Context Resolver 处理。
  </p>
  <div class="flex justify-end pt-1">
    <button type="submit" class="rounded-md bg-[var(--color-accent)] px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50" disabled={saving || !dirty}>
      {saving ? "保存中…" : "保存策略"}
    </button>
  </div>
</form>
