<script setup lang="ts">
import { ArrowDown, ArrowUp, Copy, FolderOpen, Pin } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import { isMacPlatform } from "$lib/platform";

defineProps<{
  workspaceId: string;
  workspacePath: string;
  pinned: boolean;
  orderedIndex: number;
  orderedLength: number;
  summaryLabel?: string;
  showPathCode?: boolean;
}>();

const emit = defineEmits<{
  "toggle-pin": [workspaceId: string];
  move: [workspaceId: string, direction: -1 | 1];
  reveal: [path: string];
  copy: [path: string];
}>();

const openDirLabel = isMacPlatform ? "在访达中打开" : "打开目录";
</script>

<template>
  <details class="ui-row-menu">
    <summary>{{ summaryLabel ?? "操作" }}</summary>
    <div class="ui-menu">
      <BaseButton variant="ghost" size="sm" @click="emit('toggle-pin', workspaceId)">
        <Pin :size="14" />{{ pinned ? "取消置顶" : "置顶工作区" }}
      </BaseButton>
      <BaseButton variant="ghost" size="sm" :disabled="orderedIndex <= 0" @click="emit('move', workspaceId, -1)">
        <ArrowUp :size="14" />上移
      </BaseButton>
      <BaseButton variant="ghost" size="sm" :disabled="orderedIndex < 0 || orderedIndex >= orderedLength - 1" @click="emit('move', workspaceId, 1)">
        <ArrowDown :size="14" />下移
      </BaseButton>
      <BaseButton variant="ghost" size="sm" @click="emit('reveal', workspacePath)">
        <FolderOpen :size="14" />{{ openDirLabel }}
      </BaseButton>
      <BaseButton variant="ghost" size="sm" @click="emit('copy', workspacePath)">
        <Copy :size="14" />复制绝对路径
      </BaseButton>
      <code v-if="showPathCode">{{ workspacePath }}</code>
    </div>
  </details>
</template>
