<script setup lang="ts">
import { computed, onMounted, onUnmounted } from "vue";
import { Activity, AlertTriangle, CheckCircle2, ClipboardList, GitBranch, Plus, Settings } from "@lucide/vue";
import appIcon from "$src/assets/app-icon.png";
import ThemeToggle from "$src/components/ThemeToggle.vue";
import SegmentedControl from "$src/components/ui/SegmentedControl.vue";
import { APP_VERSION } from "$lib/app-version";
import { REPO_URL } from "$lib/app-links";
import { openUrl } from "$lib/api/app-info";
import {
  globalHealthBusy,
  globalHealthLastCheckedAt,
  globalHealthSummary,
  startGlobalHealthMonitor,
  stopGlobalHealthMonitor,
} from "$lib/stores/health";
import { message } from "@tauri-apps/plugin-dialog";

defineProps<{
  dashboardActive?: boolean;
  auditActive?: boolean;
  settingsActive?: boolean;
  activeSettingsNav?: string;
}>();

const emit = defineEmits<{
  openDashboard: [];
  openAudit: [];
  addWorkspace: [];
  openSettings: [];
  settingsNavChange: [value: string];
}>();

const settingsNavItems = [
  { value: "general", label: "通用" },
  { value: "keys", label: "密钥与认证" },
  { value: "connection", label: "连接" },
];

const healthTone = computed(() => {
  if (globalHealthSummary.value.state === "healthy") return "healthy";
  if (globalHealthSummary.value.state === "warning" || globalHealthSummary.value.state === "error") return "warning";
  return "neutral";
});

const healthTime = computed(() => {
  if (!globalHealthLastCheckedAt.value) return "自动检查";
  const seconds = Math.floor((Date.now() - globalHealthLastCheckedAt.value) / 1000);
  if (seconds < 30) return "刚刚检查";
  if (seconds < 60) return `${seconds} 秒前`;
  return `${Math.floor(seconds / 60)} 分钟前`;
});

onMounted(() => startGlobalHealthMonitor());
onUnmounted(() => stopGlobalHealthMonitor());

async function openRepo() {
  try {
    await openUrl(REPO_URL);
  } catch (error) {
    await message(String(error), { title: "无法打开仓库", kind: "error" });
  }
}
</script>

<template>
  <div class="app-layout ios-app-shell">
    <aside class="ios-sidebar">
      <div class="ios-sidebar__header" data-tauri-drag-region>
        <div class="ios-sidebar__header-row flex items-start justify-between gap-2" data-tauri-drag-region>
          <button
            type="button"
            class="ios-brand btn-hover"
            :class="{ active: dashboardActive }"
            title="返回工作台"
            @click="emit('openDashboard')"
          >
            <img :src="appIcon" class="ios-brand__mark" alt="" aria-hidden="true" />
            <div class="min-w-0 text-left">
              <div class="flex items-center gap-1.5">
                <p class="ios-brand__kicker font-display tracking-widest">CODING TOOLS</p>
                <span class="inline-block h-1.5 w-1.5 rounded-full bg-[var(--primary)] shadow-[0_0_6px_var(--coral-glow)] animate-pulse" />
              </div>
              <h1 class="ios-brand__title font-display tracking-tight text-white/90">MCP Console</h1>
            </div>
          </button>
          <ThemeToggle />
        </div>
      </div>

      <div class="ios-sidebar__body">
        <button
          type="button"
          class="ios-sidebar__audit btn-hover"
          :class="{ active: auditActive }"
          title="工具调用审计"
          @click="emit('openAudit')"
        >
          <ClipboardList :size="15" :stroke-width="2" />
          <span>工具审计</span>
        </button>
        <div class="ios-sidebar__section-head">
          <p class="ios-sidebar__section-label font-display">工作区</p>
          <button class="ios-sidebar__add btn-hover" type="button" title="添加工作区" @click="emit('addWorkspace')">
            <Plus :size="13" :stroke-width="2.2" />
          </button>
        </div>
        <slot name="sidebar" />
      </div>

      <div class="ios-sidebar__footer">
        <button
          type="button"
          class="ios-sidebar__health"
          :class="`is-${healthTone}`"
          title="系统健康 · 点击返回工作台查看详情"
          @click="emit('openDashboard')"
        >
          <span class="ios-sidebar__health-icon">
            <Activity v-if="globalHealthBusy" :size="15" class="animate-pulse" />
            <CheckCircle2 v-else-if="healthTone === 'healthy'" :size="15" />
            <AlertTriangle v-else-if="healthTone === 'warning'" :size="15" />
            <Activity v-else :size="15" />
          </span>
          <span class="ios-sidebar__health-copy">
            <strong>系统健康 · {{ globalHealthSummary.label }}</strong>
            <small>{{ healthTime }}</small>
          </span>
        </button>
        <button
          type="button"
          class="ios-sidebar__settings btn-hover"
          :class="{ active: settingsActive }"
          @click="emit('openSettings')"
        >
          <Settings :size="15" :stroke-width="2" />
          <span>设置</span>
        </button>
        <div class="ios-sidebar__meta">
          <p class="font-mono">v{{ APP_VERSION }}</p>
          <button type="button" class="ios-sidebar__repo" @click="openRepo">
            <GitBranch :size="12" :stroke-width="2" />
            <span>仓库</span>
          </button>
        </div>
      </div>
    </aside>

    <main class="ios-main">
      <div v-if="settingsActive" class="tx-settings-tabs ios-glass" data-tauri-drag-region>
        <div class="tx-settings-tabs-inner">
          <SegmentedControl
            :items="settingsNavItems"
            :model-value="activeSettingsNav ?? 'general'"
            @update:model-value="emit('settingsNavChange', $event)"
          />
        </div>
      </div>
      <div v-if="settingsActive" class="ios-settings-scroll">
        <slot />
      </div>
      <slot v-else />
    </main>
  </div>
</template>

<style scoped>
.ios-app-shell {
  position: relative;
  isolation: isolate;
  background: transparent;
}

.ios-sidebar {
  position: relative;
  z-index: 5;
  width: 224px;
  flex: 0 0 224px;
  display: flex;
  min-width: 0;
  flex-direction: column;
  border-right: 1px solid var(--ui-line);
  background: var(--ui-surface);
  box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.24), inset 0 1px 0 rgba(255, 255, 255, 0.22);
  backdrop-filter: blur(var(--ui-glass-blur)) saturate(var(--ui-glass-saturation));
  -webkit-backdrop-filter: blur(var(--ui-glass-blur)) saturate(var(--ui-glass-saturation));
}

.ios-sidebar__header { padding: 14px 12px 10px; user-select: none; }
.ios-brand {
  display: flex;
  min-width: 0;
  flex: 1;
  align-items: center;
  gap: 10px;
  padding: 7px;
  border: 1px solid transparent;
  border-radius: var(--ui-radius-row);
  background: transparent;
  color: inherit;
  cursor: pointer;
  transition: background 150ms ease-out, border-color 150ms ease-out;
}
.ios-brand:hover { background: var(--ui-surface-hover); border-color: var(--ui-line); }
.ios-brand.active { background: var(--ui-accent-soft); border-color: color-mix(in srgb, var(--ui-accent) 24%, transparent); }
.ios-brand__mark {
  display: block;
  width: 32px;
  height: 32px;
  flex: 0 0 32px;
  border: 1px solid var(--ui-line);
  border-radius: 10px;
  background: var(--ui-surface-subtle);
  object-fit: cover;
  box-shadow: var(--ui-shadow-subtle);
}
.ios-brand__kicker { color: var(--ui-text-muted); font-size: 10px; font-weight: 600; letter-spacing: .08em; line-height: 1; }
.ios-brand__title { margin-top: 4px; color: var(--ui-text); font-size: 13px; font-weight: 600; line-height: 1.15; letter-spacing: -.01em; }

.ios-sidebar__body { min-height: 0; flex: 1; overflow-y: auto; padding: 8px 9px 12px; }
.ios-sidebar__audit {
  display: flex;
  width: 100%;
  min-height: 36px;
  align-items: center;
  gap: 9px;
  margin-bottom: 12px;
  padding: 7px 10px;
  border: 1px solid transparent;
  border-radius: var(--ui-radius-control);
  background: transparent;
  color: var(--ui-text-secondary);
  font-size: 13px;
  font-weight: 500;
  text-align: left;
  cursor: pointer;
  transition: background 150ms ease-out, color 150ms ease-out;
}
.ios-sidebar__audit:hover { background: var(--ui-surface-hover); color: var(--ui-text); }
.ios-sidebar__audit.active { background: var(--ui-accent-soft); color: var(--ui-accent); font-weight: 600; }
.ios-sidebar__section-head { display: flex; align-items: center; justify-content: space-between; min-height: 30px; padding: 0 5px; }
.ios-sidebar__section-label { color: var(--ui-text-muted); font-size: 11px; font-weight: 600; letter-spacing: .02em; }
.ios-sidebar__add {
  display: grid;
  width: 25px;
  height: 25px;
  place-items: center;
  border: 0;
  border-radius: var(--ui-radius-control);
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  transition: background 150ms ease-out, color 150ms ease-out;
}
.ios-sidebar__add:hover { background: var(--ui-accent-soft); color: var(--ui-accent); }

.ios-sidebar__footer { padding: 9px 10px 12px; }
.ios-sidebar__health {
  display: flex;
  width: 100%;
  min-height: 48px;
  align-items: center;
  gap: 9px;
  margin-bottom: 7px;
  padding: 8px 10px;
  border: 1px solid var(--ui-line);
  border-radius: var(--ui-radius-row);
  background: var(--ui-surface-subtle);
  color: var(--ui-text-secondary);
  text-align: left;
  cursor: pointer;
  transition: background 150ms ease-out, border-color 150ms ease-out;
}
.ios-sidebar__health:hover {
  border-color: var(--ui-line-strong);
  background: var(--ui-surface-hover);
}
.ios-sidebar__health.is-healthy {
  border-color: color-mix(in srgb, var(--ui-success) 25%, var(--ui-line));
}
.ios-sidebar__health.is-warning {
  border-color: color-mix(in srgb, var(--ui-warning) 32%, var(--ui-line));
  background: color-mix(in srgb, var(--ui-warning-soft) 58%, var(--ui-surface-subtle));
}
.ios-sidebar__health-icon {
  display: grid;
  width: 28px;
  height: 28px;
  flex: 0 0 28px;
  place-items: center;
  border-radius: 10px;
  background: var(--ui-surface-raised);
  color: var(--ui-text-muted);
}
.ios-sidebar__health.is-healthy .ios-sidebar__health-icon { color: var(--ui-success); }
.ios-sidebar__health.is-warning .ios-sidebar__health-icon { color: var(--ui-warning); }
.ios-sidebar__health-copy {
  display: grid;
  min-width: 0;
  gap: 2px;
}
.ios-sidebar__health-copy strong {
  overflow: hidden;
  color: var(--ui-text);
  font-size: 11px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.ios-sidebar__health-copy small {
  color: var(--ui-text-muted);
  font-size: 10px;
}

.ios-sidebar :focus-visible {
  outline: 2px solid var(--ui-accent);
  outline-offset: 2px;
}
.ios-sidebar__settings {
  display: flex;
  width: 100%;
  min-height: 38px;
  align-items: center;
  gap: 9px;
  padding: 7px 10px;
  border: 1px solid transparent;
  border-radius: var(--ui-radius-control);
  background: transparent;
  color: var(--ui-text-secondary);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: background 150ms ease-out, color 150ms ease-out;
}
.ios-sidebar__settings:hover { background: var(--ui-surface-hover); color: var(--ui-text); }
.ios-sidebar__settings.active { background: var(--ui-accent-soft); color: var(--ui-accent); font-weight: 600; }
.ios-sidebar__meta { display: flex; align-items: center; justify-content: space-between; margin-top: 8px; padding: 0 7px; color: var(--ui-text-muted); font-size: 11px; }
.ios-sidebar__repo { display: inline-flex; align-items: center; gap: 4px; border: 0; background: transparent; color: inherit; cursor: pointer; }
.ios-sidebar__repo:hover { color: var(--text-secondary); }

.ios-main {
  position: relative;
  z-index: 1;
  display: flex;
  min-width: 0;
  flex: 1;
  flex-direction: column;
  overflow: hidden;
  background: transparent;
  container-name: app-main;
  container-type: inline-size;
}

.ios-settings-scroll {
  min-height: 0;
  flex: 1;
  overflow-x: hidden;
  overflow-y: auto;
  overscroll-behavior: contain;
  scrollbar-gutter: stable;
  scroll-behavior: smooth;
}

:global(.ios-sidebar-workspaces) { display: grid; gap: 4px; }

:global(.dark) .ios-app-shell,
:global([data-theme="dark"]) .ios-app-shell { background: transparent; }
:global(.dark) .ios-sidebar,
:global([data-theme="dark"]) .ios-sidebar {
  border-right-color: var(--ui-line);
  background: var(--ui-surface);
  box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.08), inset 0 1px 0 rgba(255, 255, 255, 0.06);
}
:global(.dark) .ios-brand:hover,
:global([data-theme="dark"]) .ios-brand:hover,
:global(.dark) .ios-sidebar__audit:hover,
:global([data-theme="dark"]) .ios-sidebar__audit:hover,
:global(.dark) .ios-sidebar__settings:hover,
:global([data-theme="dark"]) .ios-sidebar__settings:hover {
  background: var(--ui-surface-hover);
  border-color: var(--ui-line);
}
:global(.dark) .ios-brand.active,
:global([data-theme="dark"]) .ios-brand.active,
:global(.dark) .ios-sidebar__audit.active,
:global([data-theme="dark"]) .ios-sidebar__audit.active,
:global(.dark) .ios-sidebar__settings.active,
:global([data-theme="dark"]) .ios-sidebar__settings.active {
  background: var(--ui-accent-soft);
  border-color: color-mix(in srgb, var(--ui-accent) 28%, transparent);
  box-shadow: none;
}
:global(.dark) .ios-main,
:global([data-theme="dark"]) .ios-main { background: transparent; }

@media (max-width: 1080px) {
  .ios-sidebar {
    width: 210px;
    flex-basis: 210px;
  }
}

@media (max-width: 760px) {
  .ios-sidebar {
    width: 72px;
    flex-basis: 72px;
  }

  .ios-sidebar__header {
    padding-inline: 7px;
  }

  .ios-sidebar__header-row {
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }

  .ios-brand {
    flex: 0 0 auto;
    justify-content: center;
    padding: 5px;
  }

  .ios-brand > div:last-child,
  .ios-sidebar__section-label,
  .ios-sidebar__audit span,
  .ios-sidebar__settings span,
  .ios-sidebar__health-copy,
  .ios-sidebar__meta p,
  .ios-sidebar__repo span {
    display: none;
  }

  .ios-sidebar__body {
    padding-inline: 7px;
  }

  .ios-sidebar__audit {
    min-height: 46px;
    justify-content: center;
    margin-bottom: 8px;
    padding-inline: 7px;
  }

  .ios-sidebar__section-head {
    justify-content: center;
    padding: 0;
  }

  .ios-sidebar__footer {
    padding-inline: 7px;
  }

  .ios-sidebar__health {
    min-height: 46px;
    justify-content: center;
    padding-inline: 7px;
  }

  .ios-sidebar__settings {
    justify-content: center;
    padding-inline: 7px;
  }

  .ios-sidebar__meta {
    justify-content: center;
    padding: 0;
  }

  :global(.ios-workspace-nav) {
    min-height: 46px !important;
    justify-content: center;
    gap: 0 !important;
    padding: 7px !important;
  }

  :global(.ios-workspace-nav.active) {
    min-height: 46px !important;
  }

  :global(.ios-workspace-nav__content) {
    display: none !important;
  }

  :global(.ios-workspace-nav__status) {
    position: absolute;
    top: 7px;
    right: 7px;
  }
}

@container app-main (max-width: 760px) {
  .tx-settings-tabs {
    padding-inline: 14px;
  }

  .ios-settings-scroll > :global(div) {
    max-width: 100% !important;
    padding-inline: 16px !important;
  }
}
</style>
