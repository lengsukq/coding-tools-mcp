<script setup lang="ts">
import { Folder } from "@lucide/vue";
import type { RuntimeState, WorkspaceProfile } from "$lib/types";

defineProps<{
  workspace: WorkspaceProfile;
  active?: boolean;
  mcpState: RuntimeState;
}>();

defineEmits<{ click: [] }>();
</script>

<template>
  <button
    type="button"
    class="ios-workspace-nav"
    :class="{ active }"
    :title="workspace.path"
    @click="$emit('click')"
  >
    <span class="ios-workspace-nav__icon"><Folder :size="15" :stroke-width="2" /></span>
    <span class="ios-workspace-nav__content">
      <span class="ios-workspace-nav__name">{{ workspace.name }}</span>
      <span class="ios-workspace-nav__meta">MCP · :{{ workspace.runtime.local_port }}</span>
    </span>
    <span class="ios-workspace-nav__status" :class="`is-${mcpState}`" :title="mcpState" />
  </button>
</template>

<style scoped>
.ios-workspace-nav {
  position: relative;
  display: flex;
  width: 100%;
  min-width: 0;
  min-height: 48px;
  align-items: center;
  gap: 10px;
  padding: 7px 10px;
  border: 1px solid transparent;
  border-radius: 14px;
  background: transparent;
  color: var(--text-main);
  text-align: left;
  cursor: pointer;
  transition: background 160ms ease, border-color 160ms ease, box-shadow 160ms ease, transform 160ms ease;
}

.ios-workspace-nav:hover {
  background: rgba(255, 255, 255, 0.58);
  border-color: rgba(255, 255, 255, 0.72);
}

.ios-workspace-nav:active { transform: scale(.985); }

.ios-workspace-nav.active {
  background: linear-gradient(135deg, rgba(10, 132, 255, .15), rgba(94, 92, 230, .09));
  border-color: rgba(10, 132, 255, .16);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, .72), 0 8px 22px rgba(10, 132, 255, .08);
}

.ios-workspace-nav__icon {
  display: grid;
  width: 30px;
  height: 30px;
  flex: 0 0 30px;
  place-items: center;
  border-radius: 10px;
  background: rgba(99, 102, 241, .075);
  color: #5e5ce6;
}

.active .ios-workspace-nav__icon {
  background: linear-gradient(145deg, #0a84ff, #5e5ce6);
  color: #fff;
  box-shadow: 0 5px 14px rgba(10, 132, 255, .2);
}

.ios-workspace-nav__content {
  display: block;
  min-width: 0;
  flex: 1;
}

.ios-workspace-nav__name,
.ios-workspace-nav__meta {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.ios-workspace-nav__name {
  font-size: 12px;
  font-weight: 650;
  line-height: 1.25;
  letter-spacing: -.01em;
}

.ios-workspace-nav__meta {
  margin-top: 3px;
  color: var(--text-muted);
  font-size: 9.5px;
  line-height: 1;
}

.ios-workspace-nav__status {
  width: 7px;
  height: 7px;
  flex: 0 0 7px;
  border-radius: 999px;
  background: rgba(100, 116, 139, .38);
}

.ios-workspace-nav__status.is-running {
  background: #30d158;
  box-shadow: 0 0 0 3px rgba(48, 209, 88, .11);
}

.ios-workspace-nav__status.is-starting,
.ios-workspace-nav__status.is-stopping { background: #ff9f0a; }
.ios-workspace-nav__status.is-error { background: #ff453a; }

:global(.dark) .ios-workspace-nav:hover {
  background: rgba(255,255,255,.055);
  border-color: rgba(255,255,255,.07);
}

:global(.dark) .ios-workspace-nav.active {
  background: linear-gradient(135deg, rgba(10,132,255,.18), rgba(94,92,230,.11));
  border-color: rgba(100, 180, 255, .16);
  box-shadow: inset 0 1px 0 rgba(255,255,255,.055), 0 8px 22px rgba(0,0,0,.13);
}
</style>
