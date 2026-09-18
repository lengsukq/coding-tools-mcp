<script setup lang="ts">
import { GitBranch, Plus, Settings } from "@lucide/vue";
import ThemeToggle from "$src/components/ThemeToggle.vue";
import SegmentedControl from "$src/components/ui/SegmentedControl.vue";
import { APP_VERSION } from "$lib/app-version";
import { REPO_URL } from "$lib/app-links";
import { openUrl } from "$lib/api/app-info";
import { message } from "@tauri-apps/plugin-dialog";

defineProps<{
  dashboardActive?: boolean;
  settingsActive?: boolean;
  activeSettingsNav?: string;
}>();

const emit = defineEmits<{
  openDashboard: [];
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
    <div class="ios-shell-glow ios-shell-glow--blue" />
    <div class="ios-shell-glow ios-shell-glow--violet" />

    <aside class="ios-sidebar">
      <div class="ios-sidebar__header" data-tauri-drag-region>
        <div class="ios-sidebar__header-row flex items-start justify-between gap-2" data-tauri-drag-region>
          <button
            type="button"
            class="ios-brand"
            :class="{ active: dashboardActive }"
            title="返回工作台"
            @click="emit('openDashboard')"
          >
            <div class="ios-brand__mark" aria-hidden="true">CT</div>
            <div class="min-w-0 text-left">
              <p class="ios-brand__kicker">CODING TOOLS</p>
              <h1 class="ios-brand__title">MCP Console</h1>
            </div>
          </button>
          <ThemeToggle />
        </div>
      </div>

      <div class="ios-sidebar__body">
        <div class="ios-sidebar__section-head">
          <p class="ios-sidebar__section-label">工作区</p>
          <button class="ios-sidebar__add" type="button" title="添加工作区" @click="emit('addWorkspace')">
            <Plus :size="13" :stroke-width="2.2" />
          </button>
        </div>
        <slot name="sidebar" />
      </div>

      <div class="ios-sidebar__footer">
        <button
          type="button"
          class="ios-sidebar__settings"
          :class="{ active: settingsActive }"
          @click="emit('openSettings')"
        >
          <Settings :size="15" :stroke-width="2" />
          <span>设置</span>
        </button>
        <div class="ios-sidebar__meta">
          <p>v{{ APP_VERSION }}</p>
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
  background: #f4f6fb;
}

.ios-shell-glow {
  position: absolute;
  z-index: 0;
  border-radius: 999px;
  filter: blur(95px);
  pointer-events: none;
  opacity: .13;
}
.ios-shell-glow--blue { top: -120px; left: 28%; width: 310px; height: 310px; background: #64d2ff; }
.ios-shell-glow--violet { top: 4%; right: 3%; width: 330px; height: 330px; background: #bf5af2; opacity: .095; }

.ios-sidebar {
  position: relative;
  z-index: 5;
  width: 238px;
  flex: 0 0 238px;
  display: flex;
  min-width: 0;
  flex-direction: column;
  border-right: 1px solid rgba(15, 23, 42, .055);
  background: rgba(249, 250, 253, .82);
  box-shadow: inset -1px 0 0 rgba(255,255,255,.58);
  backdrop-filter: blur(34px) saturate(175%);
  -webkit-backdrop-filter: blur(34px) saturate(175%);
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
  transition: background 160ms ease, border-color 160ms ease, transform 160ms ease;
}
.ios-brand:hover { background: rgba(255,255,255,.62); border-color: rgba(255,255,255,.7); }
.ios-brand:active { transform: scale(.985); }
.ios-brand.active { background: rgba(10,132,255,.075); }
.ios-brand__mark {
  display: grid;
  width: 32px;
  height: 32px;
  flex: 0 0 32px;
  place-items: center;
  border-radius: 10px;
  background: linear-gradient(145deg, #0a84ff 0%, #5e5ce6 100%);
  box-shadow: 0 7px 18px rgba(10,132,255,.24), inset 0 1px 0 rgba(255,255,255,.32);
  color: white;
  font-size: 11px;
  font-weight: 760;
}
.ios-brand__kicker { color: var(--text-muted); font-size: 7.5px; font-weight: 700; letter-spacing: .18em; line-height: 1; }
.ios-brand__title { margin-top: 4px; color: var(--text-main); font-size: 12.5px; font-weight: 680; line-height: 1; letter-spacing: -.015em; }

.ios-sidebar__body { min-height: 0; flex: 1; overflow-y: auto; padding: 8px 9px 12px; }
.ios-sidebar__section-head { display: flex; align-items: center; justify-content: space-between; min-height: 30px; padding: 0 5px; }
.ios-sidebar__section-label { color: var(--text-muted); font-size: 9px; font-weight: 680; letter-spacing: .06em; }
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
  transition: background 150ms ease, color 150ms ease;
}
.ios-sidebar__add:hover { background: rgba(10,132,255,.085); color: #0a84ff; }

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
}
.ios-sidebar__settings:hover { background: rgba(255,255,255,.58); }
.ios-sidebar__settings.active { background: rgba(10,132,255,.09); color: #0077ed; }
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
  background: rgba(246,248,252,.68);
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

:global(.dark) .ios-app-shell { background: #0b0d13; }
:global(.dark) .ios-sidebar {
  border-right-color: rgba(255,255,255,.055);
  background: rgba(17,20,28,.86);
  box-shadow: inset -1px 0 0 rgba(255,255,255,.025);
}
:global(.dark) .ios-brand:hover,
:global(.dark) .ios-sidebar__settings:hover { background: rgba(255,255,255,.05); border-color: rgba(255,255,255,.055); }
:global(.dark) .ios-main { background: rgba(11,14,21,.78); }

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
  .ios-sidebar__settings span,
  .ios-sidebar__meta p,
  .ios-sidebar__repo span {
    display: none;
  }

  .ios-sidebar__body {
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
