<script lang="ts">
  import type { GlobalAgentContextScanDto } from "$lib/api/agent-context";
  import type { GlobalRuntimeSettingsDto } from "$lib/api/settings";
  import { AGENT_SOURCE_OPTIONS, toggleSource } from "$lib/agent-context";
  import Button from "$lib/components/ui/Button.svelte";
  import Toggle from "$lib/components/ui/Toggle.svelte";

  interface Props {
    runtime: GlobalRuntimeSettingsDto;
    changed: boolean;
    saving: boolean;
    scan: GlobalAgentContextScanDto | null;
    scanning: boolean;
    scanError: string;
    onChange: () => void;
    onSave: () => void | Promise<void>;
    onScan: () => void | Promise<void>;
    onApplyDetected: () => void;
  }

  let {
    runtime,
    changed,
    saving,
    scan,
    scanning,
    scanError,
    onChange,
    onSave,
    onScan,
    onApplyDetected,
  }: Props = $props();

  function detection(provider: string) {
    return scan?.sources.find((source) => source.provider === provider);
  }

  function toggleInstruction(source: string, enabled: boolean) {
    runtime.instructionSources = toggleSource(runtime.instructionSources, source, enabled);
    onChange();
  }

  function toggleSkill(source: string, enabled: boolean) {
    runtime.skillSources = toggleSource(runtime.skillSources, source, enabled);
    onChange();
  }
</script>

<div class="tx-card p-5">
  <h3 class="text-sm font-semibold tracking-tight text-[var(--text-main)]">Agent Runtime</h3>
  <p class="mt-1 text-xs text-[var(--color-text-muted)]">
    为所有 Workspace 提供默认可执行 PATH 和 AI Instructions；Workspace 配置会在此基础上覆盖或追加。
  </p>
  <form class="mt-4 grid gap-3.5" onsubmit={(event) => { event.preventDefault(); void onSave(); }}>
    <div class="p-3.5 rounded-xl border border-[var(--border)] bg-[var(--card-bg)]">
      <Toggle
        bind:checked={runtime.restoreRuntimeStateOnLaunch}
        label="启动时恢复上次运行状态"
        description="默认关闭。开启后会记住哪些 Workspace 的 MCP 正在运行，并在下次启动 Coding Tools 时自动恢复。"
        onchange={onChange}
      />
      {#if runtime.restoreRuntimeStateOnLaunch}
        <p class="mt-2 text-xs text-[var(--primary)] pl-0.5">
          首次开启并保存时，会立即记录当前已经运行的服务；之后手动启动或停止都会同步更新恢复状态。
        </p>
      {/if}
    </div>

    <div class="p-3.5 rounded-xl border border-[var(--border)] bg-[var(--card-bg)]">
      <Toggle
        bind:checked={runtime.allowLanAccess}
        label="允许局域网访问"
        description="默认关闭。开启后 MCP 和 Global Gateway 会从仅监听 127.0.0.1 改为监听 0.0.0.0，同一局域网内的服务器即可访问本机端口用于内网穿透。"
        onchange={onChange}
      />
      {#if runtime.allowLanAccess}
        <p class="mt-2 text-xs text-[var(--warning)] pl-0.5">
          已开启：服务会暴露到局域网。建议同时启用认证，并确认防火墙规则符合预期。
        </p>
      {/if}
    </div>

    <label class="grid gap-1">
      <span class="text-xs text-[var(--color-text-muted)]">全局可执行 PATH（每行一个目录，也可粘贴 PATH）</span>
      <textarea
        class="min-h-24 rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-2.5 py-1.5 font-mono text-sm"
        placeholder="/opt/homebrew/bin&#10;/usr/local/bin&#10;~/.local/bin"
        bind:value={runtime.executablePaths}
        oninput={onChange}
      ></textarea>
      <span class="text-xs text-[var(--color-text-muted)]">支持绝对路径和 ~；用于查找 aws、docker、kubectl 等系统程序，仍受 Workspace 的 allowed_commands 策略约束。</span>
    </label>

    <div class="rounded-xl border border-[var(--border)] bg-[var(--surface-main)] p-4 shadow-sm">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div>
          <p class="text-sm font-semibold text-[var(--text-main)]">全局 Agent Sources</p>
          <p class="mt-1 text-xs text-[var(--color-text-muted)] leading-relaxed">
            自动扫描本机 IDE 的全局 Instructions / Skills；首次没有配置时会自动勾选检测到的来源，之后仍由用户决定是否保存或调整。
          </p>
        </div>
        <div class="flex flex-wrap items-center gap-2">
          <Button type="button" variant="secondary" size="sm" disabled={scanning} busy={scanning} onclick={onScan}>
            {scanning ? "扫描中…" : "重新扫描"}
          </Button>
          <Button
            type="button"
            variant="ghost"
            size="sm"
            disabled={!scan || scan.sources.length === 0}
            onclick={onApplyDetected}
          >
            应用检测结果
          </Button>
        </div>
      </div>
      {#if scanError}
        <p class="mt-2 text-xs text-[var(--danger)]">{scanError}</p>
      {:else if scan}
        <p class="mt-2 text-xs text-[var(--color-text-muted)]">
          已检测到 {scan.sources.length} 个 IDE / Agent 来源 · Instructions {scan.detectedInstructionSources.length} 类 · Skills {scan.detectedSkillSources.length} 类
        </p>
      {/if}

      <div class="mt-3 grid gap-3 md:grid-cols-2">
        <div>
          <p class="mb-2 text-xs font-medium">Instructions</p>
          <div class="grid gap-2">
            {#each AGENT_SOURCE_OPTIONS as option}
              {@const detected = detection(option.value)}
              <label class="flex items-start gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={runtime.instructionSources.includes(option.value)}
                  onchange={(event) => toggleInstruction(option.value, event.currentTarget.checked)}
                />
                <span>
                  <span class="block">{option.label}</span>
                  {#if detected?.instructionPaths.length}
                    <span class="block text-[11px] text-[var(--color-text-muted)]">已检测 {detected.instructionPaths.length} 个全局提示词文件</span>
                  {/if}
                </span>
              </label>
            {/each}
          </div>
        </div>
        <div>
          <p class="mb-2 text-xs font-medium">Skills</p>
          <div class="grid gap-2">
            {#each AGENT_SOURCE_OPTIONS as option}
              {@const detected = detection(option.value)}
              <label class="flex items-start gap-2 text-sm">
                <input
                  type="checkbox"
                  checked={runtime.skillSources.includes(option.value)}
                  onchange={(event) => toggleSkill(option.value, event.currentTarget.checked)}
                />
                <span>
                  <span class="block">{option.label}</span>
                  {#if detected?.skillPaths.length}
                    <span class="block text-[11px] text-[var(--color-text-muted)]">已检测 {detected.skillPaths.length} 个全局 Skill</span>
                  {/if}
                </span>
              </label>
            {/each}
          </div>
        </div>
      </div>

      {#if scan && scan.sources.length > 0}
        <details class="mt-3 text-xs">
          <summary class="cursor-pointer text-[var(--color-text-muted)]">查看全局扫描结果</summary>
          <div class="mt-2 grid gap-3">
            {#each scan.sources as source}
              <div class="rounded-md border border-[var(--color-border)] p-2.5">
                <p class="font-medium">{AGENT_SOURCE_OPTIONS.find((option) => option.value === source.provider)?.label ?? source.provider}</p>
                {#each source.instructionPaths as path}
                  <p class="mt-1 font-mono text-[11px] text-[var(--color-text-muted)]">Instruction · {path}</p>
                {/each}
                {#each source.skillPaths as path}
                  <p class="mt-1 font-mono text-[11px] text-[var(--color-text-muted)]">Skill · {path}</p>
                {/each}
              </div>
            {/each}
          </div>
        </details>
      {/if}

      {#if runtime.instructionSources.includes("custom") || runtime.skillSources.includes("custom")}
        <div class="mt-3 grid gap-3 md:grid-cols-2">
          {#if runtime.instructionSources.includes("custom")}
            <label class="grid gap-1">
              <span class="text-xs text-[var(--color-text-muted)]">Custom Instructions 文件</span>
              <textarea class="min-h-20 tx-input tx-mono" bind:value={runtime.customInstructionPaths} oninput={onChange}></textarea>
            </label>
          {/if}
          {#if runtime.skillSources.includes("custom")}
            <label class="grid gap-1">
              <span class="text-xs text-[var(--color-text-muted)]">Custom Skills 目录</span>
              <textarea class="min-h-20 tx-input tx-mono" bind:value={runtime.customSkillPaths} oninput={onChange}></textarea>
            </label>
          {/if}
        </div>
      {/if}
    </div>

    <label class="grid gap-1">
      <span class="text-xs text-[var(--color-text-muted)]">全局 AI Instructions</span>
      <textarea
        class="min-h-32 rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-2.5 py-1.5 text-sm"
        placeholder="例如：优先遵循 Clean Code；修改完成后运行相关测试。"
        bind:value={runtime.aiInstructions}
        oninput={onChange}
      ></textarea>
      <span class="text-xs text-[var(--color-text-muted)]">通过 MCP initialize.instructions 注入；Workspace Instructions 会追加在全局规则之后。</span>
    </label>

    <div class="flex justify-end pt-1">
      <Button type="submit" variant="primary" size="md" disabled={!changed || saving} busy={saving}>
        {saving ? "保存中…" : "保存 Runtime 设置"}
      </Button>
    </div>
  </form>
</div>
