<script setup lang="ts">
import { SlidersHorizontal } from "@lucide/vue";
import ModalDialog from "../ui/ModalDialog.vue";
import BaseButton from "../ui/BaseButton.vue";
import type { DashboardDensity, DashboardModuleId } from "$lib/dashboard-preferences";
defineProps<{
    open: boolean;
    density: DashboardDensity;
    hiddenModules: DashboardModuleId[];
}>();
const emit = defineEmits<{
    toggleOpen: [
    ];
    toggleDensity: [
    ];
    toggleModule: [
        id: DashboardModuleId
    ];
}>();
const modules: Array<[
    DashboardModuleId,
    string
]> = [["focus", "当前工作"], ["workspaces", "工作区"], ["activity", "最近活动"], ["usage", "用量分析"]];
</script>
<template><BaseButton variant="icon" title="工作台偏好" aria-label="工作台偏好" @click="emit('toggleOpen')">
<SlidersHorizontal :size="16"/>
</BaseButton>
<ModalDialog :open="open" title="工作台偏好" description="待验收和异常始终可见。" @close="emit('toggleOpen')">
<div class="ui-data-row">
<span>信息密度</span>
<BaseButton variant="secondary" size="sm" @click="emit('toggleDensity')">{{density==='compact'?'紧凑':'舒适'}}</BaseButton>
</div>
<div v-for="item in modules" :key="item[0]" class="ui-data-row">
<span>{{item[1]}}</span>
<BaseButton variant="ghost" size="sm" @click="emit('toggleModule',item[0])">{{hiddenModules.includes(item[0])?'显示':'隐藏'}}</BaseButton>
</div>
</ModalDialog></template>
