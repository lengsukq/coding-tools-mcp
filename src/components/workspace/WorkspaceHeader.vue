<script setup lang="ts">
import { Check, Copy, Folder, FolderOpen } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import type { WorkspaceProfile } from "$lib/types";

defineProps<{
  profile: WorkspaceProfile;
  pathCopied: boolean;
}>();

defineEmits<{
  revealDirectory: [];
  copyPath: [];
}>();
</script>

<template>
  <header class="page-header wb-workspace-header pb-4">
    <div class="workspace-header-layout">
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-3">
          <div class="grid size-11 place-items-center rounded-[18px] bg-gradient-to-br from-[#0a84ff]/18 via-[#5e5ce6]/14 to-[#bf5af2]/16 text-[var(--ios-blue)] shadow-sm">
            <Folder :size="20" :stroke-width="2.2" />
          </div>
          <div class="min-w-0">
            <h2 class="truncate text-xl font-bold tracking-tight text-[var(--text-main)]">{{ profile.name }}</h2>
            <div class="mt-1 flex items-center gap-2 text-[11px] text-[var(--text-muted)]">
              <span>Workspace Context</span><span>·</span><span>由 Global MCP 统一连接</span>
            </div>
          </div>
        </div>

        <div class="mt-3 flex flex-wrap items-center gap-2">
          <div class="ios-glass ios-inset-surface inline-flex max-w-full items-center gap-1.5 px-3 py-1.5 text-xs text-[var(--text-secondary)]">
            <span class="truncate font-mono text-[11px] select-all">{{ profile.path }}</span>
          </div>
          <BaseButton variant="ghost" size="sm" @click="$emit('revealDirectory')"><FolderOpen :size="13" />打开目录</BaseButton>
          <BaseButton variant="ghost" size="sm" @click="$emit('copyPath')">
            <Check v-if="pathCopied" :size="13" class="text-[var(--success)]" /><Copy v-else :size="13" />
            {{ pathCopied ? "已复制" : "复制路径" }}
          </BaseButton>
        </div>
      </div>

      <div class="ios-glass ios-inset-surface flex shrink-0 items-center gap-2 px-3 py-2 text-xs text-[var(--text-secondary)]">
        <span class="h-2.5 w-2.5 rounded-full bg-[#0a84ff] shadow-[0_0_0_5px_rgba(10,132,255,.10)]" />
        <span>项目级上下文</span>
      </div>
    </div>
  </header>
</template>

<style scoped>
.wb-workspace-header {
  width: 100% !important;
  max-width: none !important;
  margin-inline: 0 !important;
  padding-inline: 0 !important;
}

.workspace-header-layout {
  display: flex;
  width: 100%;
  min-width: 0;
  max-width: var(--workspace-content-max, 1640px);
  margin-inline: auto;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding-inline: var(--workspace-gutter, 28px);
  box-sizing: border-box;
}

@container workspace-page (max-width: 700px) {
  .workspace-header-layout {
    flex-direction: column;
    align-items: stretch;
  }

  .workspace-header-layout > :last-child {
    align-self: flex-start;
  }
}
</style>
