<script setup lang="ts">
import { computed, ref } from "vue";
import { Activity, ClipboardList, LayoutDashboard, Menu, Minimize2, Plus, Search, Settings, X } from "@lucide/vue";
import appIcon from "$src/assets/app-icon.png";
import ThemeToggle from "./ThemeToggle.vue";
import { APP_VERSION } from "$lib/app-meta";
import { isMacPlatform, searchShortcutLabel } from "$lib/platform";
import { globalMcpRuntimeState, workspaces } from "$lib/stores/app";
import { globalHealthSummary } from "$lib/stores/health";

defineProps<{
  dashboardActive?: boolean;
  auditActive?: boolean;
  settingsActive?: boolean;
}>();

const workspaceFilter = defineModel<string>("workspaceFilter", { default: "" });

const emit = defineEmits<{
  openDashboard: [];
  openAudit: [];
  openSearch: [];
  addWorkspace: [];
  openSettings: [];
  openHealth: [];
  compactUi: [];
}>();

const navOpen = ref(false);
const canCompact = isMacPlatform;
const shortcutHint = searchShortcutLabel;
const healthTone = computed(() =>
  globalHealthSummary.value.state === "error"
    ? "danger"
    : globalHealthSummary.value.state === "warning"
      ? "warning"
      : globalHealthSummary.value.state === "healthy"
        ? "success"
        : "neutral",
);
const mcpRunning = computed(() => globalMcpRuntimeState.value === "running");
</script>

<template>
  <div class="ui-app-shell">
    <aside class="ui-sidebar" :class="{ 'is-open': navOpen }">
      <div class="ui-brand" data-tauri-drag-region>
        <img :src="appIcon" alt="" />
        <div>
          <strong>Coding Tools MCP</strong>
          <small class="inline-flex items-center gap-1.5">
            <span
              class="h-1.5 w-1.5 rounded-full"
              :class="mcpRunning ? 'bg-[var(--ui-success)] ui-status-dot is-pulsing' : 'bg-[var(--ui-text-muted)]/60'"
            />
            <span>{{ mcpRunning ? "MCP 运行中" : "AI 开发工作台" }}</span>
          </small>
        </div>
        <ThemeToggle />
        <button
          type="button"
          class="ui-mobile-menu"
          aria-label="展开导航"
          :aria-expanded="navOpen"
          @click="navOpen = !navOpen"
        >
          <Menu :size="18" />
        </button>
      </div>

      <!-- Persistent Global Search Trigger -->
      <div class="px-1.5 pb-1.5 pt-1">
        <button
          type="button"
          aria-label="全局搜索设置与项目"
          class="flex h-8.5 w-full items-center justify-between gap-2 rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-3 text-xs text-[var(--ui-text-muted)] transition hover:border-[var(--ui-line-strong)] hover:bg-[var(--ui-surface-hover)] hover:text-[var(--ui-text)]"
          @click="emit('openSearch'); navOpen = false"
        >
          <span class="inline-flex min-w-0 items-center gap-2 truncate">
            <Search :size="13.5" class="shrink-0 text-[var(--ui-accent)]" />
            <span class="truncate">全局搜索</span>
          </span>
          <kbd class="shrink-0 rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface)] px-1.5 py-0.5 font-mono text-[10.5px] text-[var(--ui-text-muted)]">
            {{ shortcutHint }}
          </kbd>
        </button>
      </div>

      <nav class="ui-sidebar-body" aria-label="主导航">
        <button
          type="button"
          class="ui-nav-link"
          :class="{ active: dashboardActive }"
          :aria-current="dashboardActive ? 'page' : undefined"
          @click="emit('openDashboard'); navOpen = false"
        >
          <LayoutDashboard :size="17" />
          <span>工作台总览</span>
        </button>
        <button
          type="button"
          class="ui-nav-link"
          :class="{ active: auditActive }"
          :aria-current="auditActive ? 'page' : undefined"
          @click="emit('openAudit'); navOpen = false"
        >
          <ClipboardList :size="17" />
          <span>工具调用审计</span>
        </button>

        <div class="ui-sidebar-section">
          <span class="inline-flex items-center gap-1.5">
            <span>工作区</span>
            <span
              v-if="workspaces.length"
              class="rounded-full bg-[var(--ui-surface-subtle)] px-1.5 py-0.5 text-[10.5px] font-medium tabular-nums text-[var(--ui-text-muted)]"
            >
              {{ workspaces.length }}
            </span>
          </span>
          <button
            type="button"
            class="ui-icon-button h-7 w-7"
            title="添加工作区"
            aria-label="添加工作区"
            @click="emit('addWorkspace')"
          >
            <Plus :size="15" />
          </button>
        </div>

        <div v-if="workspaces.length >= 6" class="relative mb-1.5 px-1">
          <Search :size="12" class="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]" />
          <input
            v-model="workspaceFilter"
            type="text"
            aria-label="筛选侧边栏工作区"
            placeholder="筛选工作区…"
            class="h-7 w-full rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] pl-7 pr-6 text-xs text-[var(--ui-text)] outline-none transition focus:border-[var(--ui-accent)] focus:bg-[var(--ui-surface-raised)]"
          />
          <button
            v-if="workspaceFilter"
            type="button"
            class="absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)] hover:text-[var(--ui-text)]"
            aria-label="清空筛选"
            @click="workspaceFilter = ''"
          >
            <X :size="12" />
          </button>
        </div>

        <div class="space-y-0.5" @click="navOpen = false">
          <slot name="sidebar" />
        </div>
      </nav>

      <div class="ui-sidebar-footer">
        <button
          type="button"
          class="ui-health-link"
          :class="`is-${healthTone}`"
          @click="emit('openHealth'); navOpen = false"
        >
          <div class="grid h-8 w-8 shrink-0 place-items-center rounded-full bg-[var(--ui-surface)]">
            <Activity :size="15" />
          </div>
          <span class="min-w-0 flex-1">
            <strong class="truncate">连接诊断 · {{ globalHealthSummary.label }}</strong>
            <small class="truncate">点此查看网关与配对状态</small>
          </span>
        </button>

        <button
          type="button"
          class="ui-nav-link"
          :class="{ active: settingsActive }"
          @click="emit('openSettings'); navOpen = false"
        >
          <Settings :size="17" />
          <span class="flex-1">应用设置</span>
          <span class="font-mono text-[11px] text-[var(--ui-text-muted)]">v{{ APP_VERSION }}</span>
        </button>

        <button
          v-if="canCompact"
          type="button"
          class="ui-nav-link"
          title="卸载主界面以节省内存；MCP 继续运行，点击系统菜单栏图标恢复"
          @click="emit('compactUi'); navOpen = false"
        >
          <Minimize2 :size="16" />
          <span>轻量驻留</span>
        </button>
      </div>
    </aside>

    <main class="ui-main">
      <slot />
    </main>
  </div>
</template>
