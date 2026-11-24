<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { ChevronDown, ChevronRight, Search } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import ModalDialog from "$src/components/ui/ModalDialog.vue";
import type { UpstreamServerOverviewDto } from "$lib/api/upstream-mcp";

const props = defineProps<{
  server: UpstreamServerOverviewDto | null;
  saving: boolean;
}>();

const emit = defineEmits<{
  close: [];
  save: [serverId: string, disabledTools: string[]];
}>();

const searchQuery = ref("");
const disabledSet = ref<Set<string>>(new Set());
const expandedTool = ref<string | null>(null);

watch(
  () => props.server,
  (srv) => {
    searchQuery.value = "";
    expandedTool.value = null;
    disabledSet.value = new Set(srv?.config.disabledTools ?? []);
  },
  { immediate: true },
);

const filteredTools = computed(() => {
  const list = props.server?.tools ?? [];
  const q = searchQuery.value.trim().toLowerCase();
  if (!q) return list;
  return list.filter(
    (t) =>
      t.originalName.toLowerCase().includes(q) ||
      t.exposedName.toLowerCase().includes(q) ||
      t.description.toLowerCase().includes(q),
  );
});

const enabledCount = computed(() => {
  const list = props.server?.tools ?? [];
  return list.filter((t) => !disabledSet.value.has(t.originalName)).length;
});

function toggleTool(originalName: string) {
  const next = new Set(disabledSet.value);
  if (next.has(originalName)) {
    next.delete(originalName);
  } else {
    next.add(originalName);
  }
  disabledSet.value = next;
}

function enableAllFiltered() {
  const next = new Set(disabledSet.value);
  for (const t of filteredTools.value) {
    next.delete(t.originalName);
  }
  disabledSet.value = next;
}

function disableAllFiltered() {
  const next = new Set(disabledSet.value);
  for (const t of filteredTools.value) {
    next.add(t.originalName);
  }
  disabledSet.value = next;
}

function handleSave() {
  if (!props.server) return;
  emit("save", props.server.config.id, [...disabledSet.value]);
}
</script>

<template>
  <ModalDialog
    :open="Boolean(server)"
    :title="server ? `${server.config.name} · 工具过滤` : '工具过滤'"
    description="取消勾选的工具将不会暴露给 AI 客户端（对直出与代理模式同时生效），有助于减少误调用和上下文占用。"
    :dismissible="!saving"
    @close="emit('close')"
  >
    <div v-if="server" class="space-y-3">
      <div class="flex flex-wrap items-center gap-2">
        <div class="relative min-w-0 flex-1">
          <Search
            :size="13"
            class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]"
          />
          <input
            v-model="searchQuery"
            type="text"
            placeholder="筛选工具名称或说明…"
            class="h-8 w-full rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] pl-8 pr-3 text-xs text-[var(--ui-text)] outline-none focus:border-[var(--ui-accent)]"
          />
        </div>
        <BaseButton variant="secondary" size="sm" @click="enableAllFiltered">全选</BaseButton>
        <BaseButton variant="secondary" size="sm" @click="disableAllFiltered">全禁用</BaseButton>
      </div>

      <div class="max-h-[48vh] divide-y divide-[var(--ui-line)] overflow-y-auto rounded-[var(--ui-radius-control)] border border-[var(--ui-line)]">
        <div
          v-for="tool in filteredTools"
          :key="tool.originalName"
          class="p-2.5 transition-colors hover:bg-[var(--ui-surface-hover)]"
        >
          <div class="flex items-start gap-2.5">
            <input
              type="checkbox"
              class="mt-1 h-4 w-4 accent-[var(--ui-accent)]"
              :checked="!disabledSet.has(tool.originalName)"
              @change="toggleTool(tool.originalName)"
            />
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-1.5">
                <span class="font-mono text-xs font-semibold text-[var(--ui-text)]">
                  {{ server.config.exposeMode === 'direct' ? tool.exposedName : tool.originalName }}
                </span>
                <span
                  v-if="server.config.exposeMode === 'direct' && tool.exposedName !== tool.originalName"
                  class="font-mono text-[10px] text-[var(--ui-text-muted)]"
                >
                  (原名: {{ tool.originalName }})
                </span>
              </div>
              <p v-if="tool.description" class="mt-0.5 text-xs text-[var(--ui-text-secondary)]">
                {{ tool.description }}
              </p>
            </div>
            <button
              type="button"
              class="shrink-0 rounded p-1 text-[var(--ui-text-muted)] hover:text-[var(--ui-text)]"
              aria-label="展开或收起 Schema"
              @click="expandedTool = expandedTool === tool.originalName ? null : tool.originalName"
            >
              <ChevronDown v-if="expandedTool === tool.originalName" :size="14" />
              <ChevronRight v-else :size="14" />
            </button>
          </div>

          <pre
            v-if="expandedTool === tool.originalName"
            class="mt-2 max-h-40 overflow-auto rounded-md bg-[var(--ui-surface-subtle)] p-2 font-mono text-[11px] text-[var(--ui-text-secondary)]"
          >{{ JSON.stringify(tool.inputSchema, null, 2) }}</pre>
        </div>

        <p v-if="!filteredTools.length" class="p-6 text-center text-xs text-[var(--ui-text-muted)]">
          没有匹配的工具
        </p>
      </div>

      <div class="flex items-center justify-between border-t border-[var(--ui-line)] pt-3">
        <span class="text-xs text-[var(--ui-text-muted)]">
          已启用 {{ enabledCount }} / {{ server.tools.length }} 个工具
        </span>
        <div class="flex items-center gap-2">
          <BaseButton variant="secondary" :disabled="saving" @click="emit('close')">取消</BaseButton>
          <BaseButton variant="primary" :busy="saving" @click="handleSave">保存过滤</BaseButton>
        </div>
      </div>
    </div>
  </ModalDialog>
</template>
