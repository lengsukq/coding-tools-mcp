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
        <p class="ui-muted mb-1">工作区 · 项目级上下文</p>
        <div class="flex flex-wrap items-center justify-between gap-3">
          <h1 class="ui-page-header__title">{{ profile.name }}</h1>
        </div>
        <div class="mt-2.5 flex flex-wrap items-center gap-2">
          <div class="flex max-w-full items-center gap-1.5 rounded-xl border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-2.5 py-1 text-xs text-[var(--ui-text-muted)]">
            <Folder :size="13" class="shrink-0 text-[var(--ui-accent)]" />
            <code class="truncate font-mono text-[11px]">{{ profile.path }}</code>
          </div>
          <div class="flex items-center gap-1.5">
            <BaseButton variant="secondary" size="sm" @click="$emit('revealDirectory')">
              <FolderOpen :size="13" />打开目录
            </BaseButton>
            <BaseButton variant="secondary" size="sm" @click="$emit('copyPath')">
              <Check v-if="pathCopied" :size="13" class="text-[var(--ui-success)]" />
              <Copy v-else :size="13" />
              {{ pathCopied ? '已复制' : '复制路径' }}
            </BaseButton>
          </div>
        </div>
      </div>
    </div>
  </header>
</template>
<style scoped>.workspace-header-layout{width:100%;max-width:var(--workspace-content-max);margin-inline:auto;padding:28px var(--workspace-gutter) 8px;min-width:0}</style>
