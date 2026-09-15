<script lang="ts">
  import { Check, Copy, Folder, FolderOpen, HardDrive } from "@lucide/svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import StatusOrb from "$lib/components/StatusOrb.svelte";
  import type { RuntimeState, WorkspaceProfile } from "$lib/types";

  interface Props {
    profile: WorkspaceProfile;
    runtimeState: RuntimeState;
    runtimeBusy: boolean;
    pathCopied: boolean;
    onRevealDirectory: () => void | Promise<void>;
    onCopyPath: () => void;
    onToggleRuntime: () => void | Promise<void>;
  }

  let {
    profile,
    runtimeState,
    runtimeBusy,
    pathCopied,
    onRevealDirectory,
    onCopyPath,
    onToggleRuntime,
  }: Props = $props();
</script>

<header class="page-header wb-workspace-header pb-4">
  <div class="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
    <div class="min-w-0 flex-1">
      <div class="flex items-center gap-2.5">
        <div class="wb-workspace-icon flex size-9 items-center justify-center rounded-xl bg-[var(--primary-soft)] text-[var(--primary)]">
          <Folder size={18} strokeWidth={2.2} />
        </div>
        <div class="min-w-0">
          <h2 class="text-lg font-bold tracking-tight text-[var(--text-main)] truncate leading-tight">
            {profile.name}
          </h2>
        </div>
      </div>

      <div class="mt-2 flex flex-wrap items-center gap-2">
        <div class="wb-workspace-path inline-flex items-center gap-1.5 rounded-lg px-2.5 py-1 text-xs text-[var(--text-secondary)] max-w-full">
          <HardDrive size={12} class="text-[var(--text-muted)] shrink-0" />
          <span class="truncate font-mono text-[11px] select-all">{profile.path}</span>
        </div>

        <Button variant="ghost" size="sm" title="在系统访达/资源管理器中打开" onclick={onRevealDirectory}>
          <FolderOpen size={13} />
          <span>打开目录</span>
        </Button>

        <Button variant="ghost" size="sm" title="复制完整物理路径" onclick={onCopyPath}>
          {#if pathCopied}
            <Check size={13} class="text-[var(--success)]" />
            <span class="text-[var(--success)]">已复制</span>
          {:else}
            <Copy size={13} />
            <span>复制路径</span>
          {/if}
        </Button>
      </div>
    </div>

    <div class="flex flex-wrap items-center gap-2.5 shrink-0">
      <div class="wb-workspace-runtime flex items-center gap-2 rounded-xl px-3 py-1.5">
        <StatusOrb state={runtimeState} />
        <div class="text-xs">
          <span class="font-semibold text-[var(--text-main)]">MCP</span>
          <span class="text-[11px] text-[var(--text-muted)] font-mono ml-1">:{profile.runtime.local_port}</span>
        </div>
        <Button
          variant={runtimeState === "running" ? "secondary" : "primary"}
          size="sm"
          class="ml-1 px-2 py-0.5 text-[11px]"
          disabled={runtimeState === "starting" || runtimeState === "stopping"}
          busy={runtimeBusy}
          onclick={onToggleRuntime}
        >
          {runtimeState === "running" ? "停止" : "启动"}
        </Button>
      </div>
    </div>
  </div>
</header>
