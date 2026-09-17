<script setup lang="ts">
import { SlidersHorizontal } from "@lucide/vue";
import type { DashboardDensity, DashboardModuleId } from "$lib/dashboard-preferences";

defineProps<{
  open: boolean;
  density: DashboardDensity;
  hiddenModules: DashboardModuleId[];
}>();

const emit = defineEmits<{
  toggleOpen: [];
  toggleDensity: [];
  toggleModule: [moduleId: DashboardModuleId];
}>();

const moduleItems: Array<[DashboardModuleId, string]> = [
  ["focus", "当前 Focus"],
  ["attention", "需要关注"],
  ["workspaces", "工作区"],
  ["activity", "最近活动"],
  ["usage", "Token Analytics"],
  ["health", "系统健康"],
];
</script>

<template>
  <div class="wb-preferences-wrap">
    <button
      class="wb-icon-button ios-glass"
      type="button"
      title="工作台偏好"
      aria-label="工作台偏好"
      :aria-expanded="open"
      @click="emit('toggleOpen')"
    >
      <SlidersHorizontal :size="14" />
    </button>
    <div v-if="open" class="wb-preferences-popover ios-glass-strong rounded-[20px]" role="group" aria-label="工作台偏好设置">
      <p class="wb-pref-title">工作台偏好</p>
      <div class="wb-pref-row">
        <span>信息密度</span>
        <button type="button" @click="emit('toggleDensity')">{{ density === "compact" ? "紧凑" : "舒适" }}</button>
      </div>
      <div v-for="item in moduleItems" :key="item[0]" class="wb-pref-row">
        <span>{{ item[1] }}</span>
        <button type="button" @click="emit('toggleModule', item[0])">
          {{ hiddenModules.includes(item[0]) ? "显示" : "隐藏" }}
        </button>
      </div>
    </div>
  </div>
</template>
