<script setup lang="ts">
import { SlidersHorizontal } from "@lucide/vue";
import ModalDialog from "../ui/ModalDialog.vue";
import BaseButton from "../ui/BaseButton.vue";
import SegmentedControl from "../ui/SegmentedControl.vue";
import ToggleSwitch from "../ui/ToggleSwitch.vue";
import type { DashboardDensity, DashboardModuleId } from "$lib/dashboard-preferences";

defineProps<{
  open: boolean;
  density: DashboardDensity;
  hiddenModules: DashboardModuleId[];
}>();
const emit = defineEmits<{
  toggleOpen: [];
  toggleDensity: [];
  toggleModule: [id: DashboardModuleId];
}>();

const modules: Array<[DashboardModuleId, string, string]> = [
  ["focus", "当前焦点工作", "展示最近活跃工作区的目标、计划与步骤进度"],
  ["workspaces", "工作区矩阵", "在首页集中查看所有项目的任务、分支与变更数"],
  ["activity", "最近开发活动", "汇总各工作区最近归档的 AI 开发会话摘要"],
  ["usage", "传输用量分析", "按需估算 MCP 请求数、工具调用次数与 Token 趋势"],
];
</script>

<template>
  <BaseButton variant="secondary" size="sm" title="工作台显示偏好" aria-label="工作台偏好" @click="emit('toggleOpen')">
    <SlidersHorizontal :size="14" />
    <span>视图偏好</span>
  </BaseButton>
  <ModalDialog :open="open" title="工作台显示偏好" description="自定义首页 Bento 模块与排版密度；待验收与异常提醒始终置顶保留。" @close="emit('toggleOpen')">
    <div class="space-y-4">
      <div class="ui-inset flex items-center justify-between gap-4 p-4">
        <div>
          <strong class="block text-sm font-medium text-[var(--ui-text)]">排版信息密度</strong>
          <span class="mt-0.5 block text-xs text-[var(--ui-text-muted)]">紧凑模式适合多工作区同屏巡检</span>
        </div>
        <SegmentedControl
          :model-value="density"
          :items="[
            { value: 'comfortable', label: '舒适留白' },
            { value: 'compact', label: '紧凑巡检' },
          ]"
          aria-label="信息密度"
          @update:model-value="$event !== density && emit('toggleDensity')"
        />
      </div>

      <div class="ui-inset divide-y divide-[var(--ui-line)] px-4 py-1">
        <div v-for="item in modules" :key="item[0]" class="py-3">
          <ToggleSwitch
            :model-value="!hiddenModules.includes(item[0])"
            :label="item[1]"
            :description="item[2]"
            @update:model-value="emit('toggleModule', item[0])"
          />
        </div>
      </div>
    </div>
  </ModalDialog>
</template>
