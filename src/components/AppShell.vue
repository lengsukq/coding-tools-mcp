<script setup lang="ts">
import { ClipboardList, GitBranch, Plus, Settings } from "@lucide/vue";
import ThemeToggle from "$src/components/ThemeToggle.vue";
import SegmentedControl from "$src/components/ui/SegmentedControl.vue";
import { APP_VERSION } from "$lib/app-version";
import { REPO_URL } from "$lib/app-links";
import { openUrl } from "$lib/api/app-info";
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
            <div class="ios-brand__mark shadow-sm" aria-hidden="true">CT</div>
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
  width: 238px;
  flex: 0 0 238px;
  display: flex;
  min-width: 0;
  flex-direction: column;
  border-right: 1px solid var(--sidebar-border);
  background: var(--sidebar-bg);
  box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.4);
  backdrop-filter: blur(34px) saturate(180%);
  -webkit-backdrop-filter: blur(34px) saturate(180%);
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
  border-radius: 14px;
  background: transparent;
  color: inherit;
  cursor: pointer;
  transition: background 180ms var(--ease-apple-spring), border-color 180ms var(--ease-apple-spring), transform 180ms var(--ease-apple-spring);
}
.ios-brand:hover { background: rgba(255, 255, 255, 0.7); border-color: rgba(255, 255, 255, 0.8); transform: scale(1.01); }
.ios-brand:active { transform: scale(0.975); }
.ios-brand.active { background: var(--primary-soft, rgba(var(--pal-rgb, 226, 87, 76), 0.1)); border-color: var(--card-border-active, rgba(var(--pal-rgb, 226, 87, 76), 0.25)); }
.ios-brand__mark {
  display: grid;
  width: 32px;
  height: 32px;
  flex: 0 0 32px;
  place-items: center;
  border-radius: 10px;
  background: var(--primary-gradient, var(--accent-gradient));
  box-shadow: 0 5px 16px var(--coral-glow, rgba(226, 87, 76, 0.32)), inset 0 1px 0 rgba(255, 255, 255, 0.5);
  color: white;
  font-size: 11px;
  font-weight: 780;
}
.ios-brand__kicker { color: var(--text-muted); font-size: 7.5px; font-weight: 700; letter-spacing: .18em; line-height: 1; }
.ios-brand__title { margin-top: 4px; color: var(--text-main); font-size: 12.5px; font-weight: 700; line-height: 1; letter-spacing: -.02em; }

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
  border-radius: 12px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 11.5px;
  font-weight: 560;
  text-align: left;
  cursor: pointer;
  transition: all 180ms var(--ease-apple-spring);
}
.ios-sidebar__audit:hover { background: rgba(255, 255, 255, 0.65); color: var(--text-main); }
.ios-sidebar__audit.active { background: var(--primary-soft, rgba(var(--pal-rgb, 226, 87, 76), 0.1)); color: var(--primary, #e2574c); font-weight: 600; }
.ios-sidebar__section-head { display: flex; align-items: center; justify-content: space-between; min-height: 30px; padding: 0 5px; }
.ios-sidebar__section-label { color: var(--text-muted); font-size: 9px; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
.ios-sidebar__add {
  display: grid;
  width: 25px;
  height: 25px;
  place-items: center;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: var(--text-muted);
  cursor: pointer;
  transition: background 180ms var(--ease-apple-spring), color 180ms var(--ease-apple-spring), transform 180ms var(--ease-apple-spring);
}
.ios-sidebar__add:hover { background: var(--primary-soft, rgba(var(--pal-rgb, 226, 87, 76), 0.12)); color: var(--primary, #e2574c); transform: scale(1.08); }
.ios-sidebar__add:active { transform: scale(0.92); }

.ios-sidebar__footer { padding: 9px 10px 12px; }
.ios-sidebar__settings {
  display: flex;
  width: 100%;
  min-height: 38px;
  align-items: center;
  gap: 9px;
  padding: 7px 10px;
  border: 1px solid transparent;
  border-radius: 12px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 11.5px;
  font-weight: 560;
  cursor: pointer;
  transition: all 180ms var(--ease-apple-spring);
}
.ios-sidebar__settings:hover { background: rgba(255, 255, 255, 0.65); color: var(--text-main); }
.ios-sidebar__settings.active { background: var(--primary-soft, rgba(var(--pal-rgb, 226, 87, 76), 0.1)); color: var(--primary, #e2574c); font-weight: 600; }
.ios-sidebar__meta { display: flex; align-items: center; justify-content: space-between; margin-top: 8px; padding: 0 7px; color: var(--text-muted); font-size: 9px; }
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
  border-right-color: var(--sidebar-border, rgba(255, 255, 255, 0.08));
  background: var(--sidebar-bg, rgba(14, 14, 18, 0.72));
  backdrop-filter: blur(28px) saturate(1.4);
  -webkit-backdrop-filter: blur(28px) saturate(1.4);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.08), inset -1px 0 0 rgba(255, 255, 255, 0.04), 0 20px 60px rgba(0, 0, 0, 0.4);
}
:global(.dark) .ios-brand:hover,
:global([data-theme="dark"]) .ios-brand:hover,
:global(.dark) .ios-sidebar__audit:hover,
:global([data-theme="dark"]) .ios-sidebar__audit:hover,
:global(.dark) .ios-sidebar__settings:hover,
:global([data-theme="dark"]) .ios-sidebar__settings:hover {
  background: rgba(255, 255, 255, 0.08);
  border-color: rgba(255, 255, 255, 0.12);
}
:global(.dark) .ios-brand.active,
:global([data-theme="dark"]) .ios-brand.active,
:global(.dark) .ios-sidebar__audit.active,
:global([data-theme="dark"]) .ios-sidebar__audit.active,
:global(.dark) .ios-sidebar__settings.active,
:global([data-theme="dark"]) .ios-sidebar__settings.active {
  background: var(--primary-soft, rgba(var(--pal-rgb, 226, 87, 76), 0.16));
  border-color: var(--card-border-active, rgba(var(--pal-rgb, 226, 87, 76), 0.35));
  box-shadow: 0 0 18px rgba(var(--pal-rgb, 226, 87, 76), 0.22);
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
