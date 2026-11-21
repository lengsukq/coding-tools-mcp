<script setup lang="ts">
import { Check, Copy, FolderGit2, FolderOpen } from "@lucide/vue";
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
      <div class="ui-surface flex flex-wrap items-center justify-between gap-4 px-6 py-5">
        <div class="flex min-w-0 flex-1 items-center gap-3.5">
          <div class="grid h-12 w-12 shrink-0 place-items-center rounded-[16px] border border-[var(--ui-accent)]/20 bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]">
            <FolderGit2 :size="22" />
          </div>
          <div class="min-w-0 flex-1">
            <p class="ui-page-header__kicker !mb-0.5">工作区 · 项目级上下文</p>
            <h1 class="truncate text-2xl font-bold tracking-tight text-[var(--ui-text)]">{{ profile.name }}</h1>
            <div class="mt-1.5 flex items-center gap-2 text-xs text-[var(--ui-text-muted)]">
              <code class="truncate rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-2.5 py-0.5 font-mono text-[11px] text-[var(--ui-text-secondary)]">
                {{ profile.path }}
              </code>
            </div>
          </div>
        </div>

        <div class="flex shrink-0 flex-wrap items-center gap-2">
          <BaseButton variant="secondary" size="sm" @click="$emit('revealDirectory')">
            <FolderOpen :size="14" />
            <span>打开目录</span>
          </BaseButton>
          <BaseButton variant="secondary" size="sm" @click="$emit('copyPath')">
            <Check v-if="pathCopied" :size="14" class="text-[var(--ui-success)]" />
            <Copy v-else :size="14" />
            <span>{{ pathCopied ? "已复制" : "复制路径" }}</span>
          </BaseButton>
        </div>
      </div>
    </div>
  </header>
</template>

<style scoped>
.workspace-header-layout {
  width: 100%;
  max-width: var(--workspace-content-max);
  margin-inline: auto;
  padding: 22px var(--workspace-gutter) 4px;
  min-width: 0;
}
</style>
