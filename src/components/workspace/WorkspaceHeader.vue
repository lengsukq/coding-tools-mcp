<script setup lang="ts">
import { Check, Copy, Folder, FolderOpen, HardDrive, Play, Square } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import type { RuntimeState, WorkspaceProfile } from "$lib/types";
import { stateLabel } from "$lib/workspace-page";

defineProps<{
  profile: WorkspaceProfile;
  runtimeState: RuntimeState;
  runtimeBusy: boolean;
  pathCopied: boolean;
}>();

defineEmits<{
  revealDirectory: [];
  copyPath: [];
  toggleRuntime: [];
}>();
</script>

<template>
  <header class="page-header wb-workspace-header pb-4">
    <div class="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-3">
          <div class="grid size-11 place-items-center rounded-[18px] bg-gradient-to-br from-[#0a84ff]/18 via-[#5e5ce6]/14 to-[#bf5af2]/16 text-[var(--ios-blue)] shadow-sm">
            <Folder :size="20" :stroke-width="2.2" />
          </div>
          <div class="min-w-0">
            <h2 class="truncate text-xl font-bold tracking-tight text-[var(--text-main)]">{{ profile.name }}</h2>
            <div class="mt-1 flex items-center gap-2 text-[11px] text-[var(--text-muted)]">
              <span class="inline-flex items-center gap-1.5"><HardDrive :size="11" /> :{{ profile.runtime.local_port }}</span>
              <span>·</span>
              <span>{{ stateLabel(runtimeState) }}</span>
            </div>
          </div>
        </div>

        <div class="mt-3 flex flex-wrap items-center gap-2">
          <div class="ios-glass ios-inset-surface inline-flex max-w-full items-center gap-1.5 px-3 py-1.5 text-xs text-[var(--text-secondary)]">
            <HardDrive :size="12" class="shrink-0 text-[var(--text-muted)]" />
            <span class="truncate font-mono text-[11px] select-all">{{ profile.path }}</span>
          </div>
          <BaseButton variant="ghost" size="sm" @click="$emit('revealDirectory')"><FolderOpen :size="13" />打开目录</BaseButton>
          <BaseButton variant="ghost" size="sm" @click="$emit('copyPath')">
            <Check v-if="pathCopied" :size="13" class="text-[var(--success)]" /><Copy v-else :size="13" />
            {{ pathCopied ? "已复制" : "复制路径" }}
          </BaseButton>
        </div>
      </div>

      <div class="ios-glass ios-inset-surface flex shrink-0 items-center gap-3 px-3 py-2">
        <span class="h-2.5 w-2.5 rounded-full" :class="runtimeState === 'running' ? 'bg-[#30d158] shadow-[0_0_0_5px_rgba(48,209,88,.12)]' : runtimeState === 'error' ? 'bg-[#ff453a]' : 'bg-black/20 dark:bg-white/20'" />
        <div class="text-xs"><strong>MCP</strong><span class="ml-1 font-mono text-[10px] text-[var(--text-muted)]">:{{ profile.runtime.local_port }}</span></div>
        <BaseButton
          :variant="runtimeState === 'running' ? 'danger' : 'primary'"
          size="sm"
          :busy="runtimeBusy"
          :disabled="runtimeState === 'starting' || runtimeState === 'stopping'"
          @click="$emit('toggleRuntime')"
        >
          <Square v-if="runtimeState === 'running'" :size="12" /><Play v-else :size="12" />
          {{ runtimeState === "running" ? "停止" : "启动" }}
        </BaseButton>
      </div>
    </div>
  </header>
</template>
