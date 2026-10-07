<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { Activity, ClipboardList, LayoutDashboard, Plus, Settings, Menu } from "@lucide/vue";
import appIcon from "$src/assets/app-icon.png";
import ThemeToggle from "./ThemeToggle.vue";
import { APP_VERSION } from "$lib/app-version";
import { globalHealthSummary, startGlobalHealthMonitor, stopGlobalHealthMonitor } from "$lib/stores/health";
defineProps<{
    dashboardActive?: boolean;
    auditActive?: boolean;
    settingsActive?: boolean;
}>();
const emit = defineEmits<{
    openDashboard: [
    ];
    openAudit: [
    ];
    addWorkspace: [
    ];
    openSettings: [
    ];
    openHealth: [
    ];
}>();
const navOpen = ref(false);
const healthTone = computed(() => globalHealthSummary.value.state === 'error' ? 'danger' : globalHealthSummary.value.state === 'warning' ? 'warning' : globalHealthSummary.value.state === 'healthy' ? 'success' : 'neutral');
onMounted(startGlobalHealthMonitor);
onUnmounted(stopGlobalHealthMonitor);
</script>
<template><div class="ui-app-shell">
<aside class="ui-sidebar" :class="{'is-open':navOpen}">
 <div class="ui-brand" data-tauri-drag-region>
<img :src="appIcon" alt="" />
<div>
<strong>Coding Tools MCP</strong>
<small>AI 开发工作台</small>
</div>
<ThemeToggle />
<button type="button" class="ui-mobile-menu" aria-label="展开导航" :aria-expanded="navOpen" @click="navOpen=!navOpen">
<Menu :size="18"/>
</button>
</div>
 <nav class="ui-sidebar-body" aria-label="主导航">
  <button type="button" class="ui-nav-link" :class="{active:dashboardActive}" :aria-current="dashboardActive?'page':undefined" @click="emit('openDashboard');navOpen=false">
<LayoutDashboard :size="17"/>工作台</button>
  <button type="button" class="ui-nav-link" :class="{active:auditActive}" :aria-current="auditActive?'page':undefined" @click="emit('openAudit');navOpen=false">
<ClipboardList :size="17"/>工具审计</button>
  <div class="ui-sidebar-section">
<span>工作区</span>
<button type="button" class="ui-icon-button" title="添加工作区" aria-label="添加工作区" @click="emit('addWorkspace')">
<Plus :size="16"/>
</button>
</div>
  <div @click="navOpen=false"><slot name="sidebar" /></div>
 </nav>
 <div class="ui-sidebar-footer">
<button type="button" class="ui-health-link" :class="`is-${healthTone}`" @click="emit('openHealth');navOpen=false">
<Activity :size="17"/>
<span>
<strong>系统健康 · {{globalHealthSummary.label}}</strong>
<small>查看连接详情</small>
</span>
</button>
<button type="button" class="ui-nav-link" :class="{active:settingsActive}" @click="emit('openSettings');navOpen=false">
<Settings :size="17"/>应用设置</button>
<small class="ui-muted">v{{APP_VERSION}}</small>
</div>
 </aside>
<main class="ui-main">
<slot />
</main>
</div></template>
