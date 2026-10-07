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

<template><header class="workspace-page-header"><div class="workspace-header-layout"><div class="min-w-0 flex-1"><p class="ui-muted mb-1">工作区 · 项目级上下文</p><h1 class="ui-page-header__title">{{profile.name}}</h1><details class="mt-3"><summary class="ui-muted">目录与操作</summary><div class="ui-actions mt-2"><code class="ui-muted break-all">{{profile.path}}</code><BaseButton variant="secondary" size="sm" @click="$emit('revealDirectory')"><FolderOpen :size="14"/>打开目录</BaseButton><BaseButton variant="secondary" size="sm" @click="$emit('copyPath')"><Check v-if="pathCopied" :size="14"/><Copy v-else :size="14"/>{{pathCopied?'已复制':'复制路径'}}</BaseButton></div></details></div></div></header></template>
<style scoped>.workspace-header-layout{width:100%;max-width:var(--workspace-content-max);margin-inline:auto;padding:28px var(--workspace-gutter) 8px;min-width:0}</style>
