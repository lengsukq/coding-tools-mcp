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
  <header class="workspace-page-header">
    <div class="workspace-header-layout">
      <div class="min-w-0 flex-1">
        <div class="flex items-center gap-3.5">
          <div class="grid size-10 shrink-0 place-items-center rounded-lg bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]">
            <Folder :size="20" :stroke-width="2.2" />
          </div>
          <div class="min-w-0">
            <h1 class="truncate text-2xl font-semibold tracking-[-0.025em] text-[var(--ui-text)]">{{ profile.name }}</h1>
            <div class="mt-1 flex items-center gap-2 text-xs text-[var(--ui-text-muted)]">
              <span>Workspace Context</span><span>·</span><span>由 Global MCP 统一连接</span>
            </div>
          </div>
        </div>

        <div class="mt-3.5 flex flex-wrap items-center gap-2">
          <div class="ui-inset inline-flex max-w-full items-center gap-1.5 px-3 py-1.5 text-xs text-[var(--ui-text-secondary)]">
            <span class="truncate font-mono text-xs select-all">{{ profile.path }}</span>
          </div>
          <BaseButton variant="secondary" size="sm" @click="$emit('revealDirectory')"><FolderOpen :size="13" />打开目录</BaseButton>
          <BaseButton variant="secondary" size="sm" @click="$emit('copyPath')">
            <Check v-if="pathCopied" :size="13" class="text-[var(--success)]" /><Copy v-else :size="13" />
            {{ pathCopied ? "已复制" : "复制路径" }}
          </BaseButton>
        </div>
      </div>

      <div class="ui-inset flex shrink-0 items-center gap-2 px-3 py-2 text-xs font-medium text-[var(--ui-text-secondary)]">
        <span class="h-2 w-2 rounded-full bg-[var(--ui-accent)]" />
        <span>项目级上下文</span>
      </div>
    </div>
  </header>
</template>

<style scoped>
.workspace-page-header {
  width: 100% !important;
  max-width: none !important;
  margin-inline: 0 !important;
  padding: 0 !important;
  border-bottom: 1px solid var(--ui-line);
}

.workspace-header-layout {
  display: flex;
  width: 100%;
  min-width: 0;
  max-width: var(--workspace-content-max, var(--ui-page-max));
  margin-inline: auto;
  align-items: center;
  justify-content: space-between;
  gap: 1rem;
  padding: 24px var(--workspace-gutter, var(--ui-page-gutter)) 18px;
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
