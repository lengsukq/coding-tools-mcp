<script lang="ts">
  import { scanAgentContext, type AgentContextSnapshotDto } from "$lib/api/agent-context";
  import { AGENT_SOURCE_OPTIONS, toggleSource } from "$lib/agent-context";
  import Button from "$lib/components/ui/Button.svelte";
  import Select from "$lib/components/ui/Select.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";
  import Toggle from "$lib/components/ui/Toggle.svelte";

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
    { value: "compact", label: "精简开发（推荐）" },
    { value: "core", label: "兼容核心" },
    { value: "advanced", label: "完整工具" },
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
  <label class="grid gap-1.5">
    <span class="text-xs font-medium text-[var(--color-text-muted)]">工具档位</span>
    <Select
      options={TOOL_PROFILE_OPTIONS}
      bind:value={draftProfile}
    />
  </label>

  <div class="grid gap-1.5">
    <span class="text-xs font-medium text-[var(--color-text-muted)]">系统命令（逗号分隔）</span>
    <TextInput
      mono
      placeholder="python,git,gh,aws,cargo,..."
      bind:value={draftCommands}
    />
  </div>

  <div class="grid gap-1.5">
    <span class="text-xs font-medium text-[var(--color-text-muted)]">额外可执行 PATH（每行一个目录，也可粘贴 PATH）</span>
    <textarea
      class="min-h-20 w-full px-3 py-2 text-xs font-mono rounded-lg border border-[var(--border)] bg-[var(--card-bg)] text-[var(--text-main)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--primary)]/25 focus:border-[var(--primary)] transition-all"
      placeholder="/opt/homebrew/bin&#10;/usr/local/bin&#10;~/.cargo/bin"
      bind:value={draftExecutablePaths}
    ></textarea>
    <span class="text-[11px] text-[var(--color-text-muted)]">Workspace PATH 优先于 Global PATH，再回退 System PATH；命令仍需加入白名单。</span>
  </div>

  <div class="rounded-xl border border-[var(--border)] bg-[var(--surface-main)] p-4 shadow-sm">
    <div class="mb-3.5">
      <p class="text-sm font-semibold text-[var(--text-main)]">Agent Context Sources</p>
      <p class="mt-1 text-xs text-[var(--color-text-muted)] leading-relaxed">
        Workspace 未选择来源时继承全局设置；compact 工具档只常驻根目录核心规则，其余规则按需读取。Skills 会在 list/get 时实时重扫。
      </p>
    </div>

    <div class="grid gap-4 md:grid-cols-2">
      <div class="space-y-2.5">
        <p class="text-xs font-semibold text-[var(--text-secondary)] uppercase tracking-wider">Instructions</p>
        <div class="grid gap-2">
          {#each AGENT_SOURCE_OPTIONS as option}
            <label class="flex items-start gap-2.5 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] p-2.5 text-xs cursor-pointer hover:border-[var(--border-strong)] transition-all">
              <input
                type="checkbox"
                class="mt-0.5 rounded border-[var(--border)] text-[var(--primary)] focus:ring-[var(--primary)]/30"
                checked={draftInstructionSources.includes(option.value)}
                onchange={(event) => {
                  draftInstructionSources = toggleSource(
                    draftInstructionSources,
                    option.value,
                    event.currentTarget.checked,
                  );
                }}
              />
              <span class="flex-1 min-w-0">
                <span class="block font-medium text-[var(--text-main)]">{option.label}</span>
                <span class="block text-[11px] text-[var(--color-text-muted)] mt-0.5">{option.detail}</span>
              </span>
            </label>
          {/each}
        </div>
      </div>

      <div class="space-y-2.5">
        <p class="text-xs font-semibold text-[var(--text-secondary)] uppercase tracking-wider">Skills</p>
        <div class="grid gap-2">
          {#each AGENT_SOURCE_OPTIONS as option}
            <label class="flex items-start gap-2.5 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] p-2.5 text-xs cursor-pointer hover:border-[var(--border-strong)] transition-all">
              <input
                type="checkbox"
                class="mt-0.5 rounded border-[var(--border)] text-[var(--primary)] focus:ring-[var(--primary)]/30"
                checked={draftSkillSources.includes(option.value)}
                onchange={(event) => {
                  draftSkillSources = toggleSource(
                    draftSkillSources,
                    option.value,
                    event.currentTarget.checked,
                  );
                }}
              />
              <span class="flex-1 min-w-0">
                <span class="block font-medium text-[var(--text-main)]">{option.label}</span>
                <span class="block text-[11px] text-[var(--color-text-muted)] mt-0.5">{option.detail}</span>
              </span>
            </label>
          {/each}
        </div>
      </div>
    </div>

    {#if draftInstructionSources.includes("custom") || draftSkillSources.includes("custom")}
      <div class="mt-4 grid gap-3 md:grid-cols-2 pt-3 border-t border-[var(--border)]">
        {#if draftInstructionSources.includes("custom")}
          <div class="grid gap-1.5">
            <span class="text-xs font-medium text-[var(--color-text-muted)]">自定义 Instructions 文件（每行一个）</span>
            <textarea
              class="min-h-20 w-full px-3 py-2 text-xs font-mono rounded-lg border border-[var(--border)] bg-[var(--card-bg)] text-[var(--text-main)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--primary)]/25 focus:border-[var(--primary)] transition-all"
              placeholder="docs/AI_RULES.md"
              bind:value={draftCustomInstructionPaths}
            ></textarea>
          </div>
        {/if}
        {#if draftSkillSources.includes("custom")}
          <div class="grid gap-1.5">
            <span class="text-xs font-medium text-[var(--color-text-muted)]">自定义 Skills 根目录（每行一个）</span>
            <textarea
              class="min-h-20 w-full px-3 py-2 text-xs font-mono rounded-lg border border-[var(--border)] bg-[var(--card-bg)] text-[var(--text-main)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--primary)]/25 focus:border-[var(--primary)] transition-all"
              placeholder=".my-agent/skills"
              bind:value={draftCustomSkillPaths}
            ></textarea>
          </div>
        {/if}
      </div>
    {/if}

    <div class="mt-4 flex flex-wrap items-center gap-3 pt-3 border-t border-[var(--border)]">
      <Button
        type="button"
        variant="secondary"
        size="sm"
        disabled={scanning}
        busy={scanning}
        onclick={() => void scan()}
      >
        {scanning ? "扫描中…" : "立即重新扫描"}
      </Button>
      {#if scanResult}
        <span class="text-xs text-[var(--color-text-muted)]">
          已发现 <strong class="text-[var(--text-main)]">{scanResult.instructions.length}</strong> 个 Instructions · <strong class="text-[var(--text-main)]">{scanResult.skills.length}</strong> 个 Skills
        </span>
      {/if}
    </div>
    {#if scanError}<p class="mt-2 text-xs text-[var(--danger)]">{scanError}</p>{/if}
    {#if scanResult && (scanResult.instructions.length > 0 || scanResult.skills.length > 0)}
      <details class="mt-3 text-xs">
        <summary class="cursor-pointer text-[var(--color-text-muted)] hover:text-[var(--text-main)]">查看发现结果</summary>
        <div class="mt-2 grid gap-1.5 rounded-lg bg-[var(--card-bg)] p-3 border border-[var(--border)] font-mono text-[11px]">
          {#each scanResult.instructions as item}
            <div><strong class="text-[var(--primary)]">[{item.provider}]</strong> {item.path}</div>
          {/each}
          {#each scanResult.skills as skill}
            <div><strong class="text-[var(--accent)]">Skill · {skill.name}</strong> · {skill.provider} · {skill.path}</div>
          {/each}
        </div>
      </details>
    {/if}
  </div>

  <div class="grid gap-1.5">
    <span class="text-xs font-medium text-[var(--color-text-muted)]">Workspace AI Instructions</span>
    <textarea
      class="min-h-28 w-full px-3 py-2 text-xs rounded-lg border border-[var(--border)] bg-[var(--card-bg)] text-[var(--text-main)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--primary)]/25 focus:border-[var(--primary)] transition-all"
      placeholder="例如：修改代码必须遵循 Clean Code；运行测试后再确认完成。"
      bind:value={draftAiInstructions}
    ></textarea>
    <span class="text-[11px] text-[var(--color-text-muted)]">手动规则会与 Global Instructions、compact 模式选中的核心 Repository Instructions 一起注入。</span>
  </div>

  <div class="rounded-lg border border-[var(--border)] bg-[var(--card-bg)] p-3">
    <Toggle
      bind:checked={draftLocalEntries}
      label="允许执行 Workspace 内本地入口"
      description="开启后允许通过本地相对路径直接执行工作区内部的工具与脚本"
    />
  </div>

  <div class="grid gap-1.5">
    <span class="text-xs font-medium text-[var(--color-text-muted)]">本地脚本扩展名（逗号分隔）</span>
    <TextInput
      mono
      placeholder=".exe,.bat,.cmd,.ps1"
      bind:value={draftExtensions}
      disabled={!draftLocalEntries}
    />
  </div>

  <label class="grid gap-1.5">
    <span class="text-xs font-medium text-[var(--color-text-muted)]">权限模式</span>
    <Select
      options={PERMISSION_MODE_OPTIONS}
      bind:value={draftMode}
    />
  </label>
  <p class="text-xs text-[var(--color-text-muted)]">
    P0 仅自动注入 repository-wide / always-apply 规则；按文件 glob、生效目录等动态 scoped rules 后续由 Context Resolver 处理。
  </p>
  <div class="flex justify-end pt-1">
    <Button
      type="submit"
      variant="primary"
      busy={saving}
      disabled={saving || !dirty}
    >
      {saving ? "保存中…" : "保存策略"}
    </Button>
  </div>
</form>
