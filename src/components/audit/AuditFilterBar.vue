<script setup lang="ts">
import { ChevronDown, Filter, RotateCcw, Search, X } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import SegmentedControl from "$src/components/ui/SegmentedControl.vue";
import type { WorkspaceProfile } from "$lib/types";

defineProps<{
  workspaces: WorkspaceProfile[];
  outcomeSegments: Array<{ value: string; label: string }>;
  timePresets: ReadonlyArray<{ value: string; label: string }>;
  activeFilters: Array<{ id: string; label: string; clear: () => void }>;
  timePreset: string;
}>();

const query = defineModel<string>("query", { required: true });
const outcome = defineModel<string>("outcome", { required: true });
const workspaceId = defineModel<string>("workspaceId", { required: true });
const toolName = defineModel<string>("toolName", { required: true });
const sessionId = defineModel<string>("sessionId", { required: true });
const fromDate = defineModel<string>("fromDate", { required: true });
const toDate = defineModel<string>("toDate", { required: true });
const showAdvancedFilters = defineModel<boolean>("showAdvancedFilters", { required: true });

const emit = defineEmits<{
  apply: [];
  reset: [];
  "select-time-preset": [preset: string];
}>();
</script>

<template>
  <div class="space-y-3.5 border-b border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] p-5">
    <div class="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
      <div class="relative min-w-[240px] max-w-xl flex-1">
        <Search :size="15" class="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]" />
        <input
          v-model="query"
          type="text"
          aria-label="搜索审计记录"
          placeholder="搜索工具名称、Request ID、Session ID、客户端…"
          class="h-10 w-full rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] pl-10 pr-9 text-xs text-[var(--ui-text)] placeholder-[var(--ui-text-muted)] outline-none transition focus:border-[var(--ui-accent)] focus:shadow-[var(--focus-ring)]"
          @keydown.enter="emit('apply')"
        />
        <button
          v-if="query"
          type="button"
          class="absolute right-3 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)] hover:text-[var(--ui-text)]"
          @click="query = ''; emit('apply')"
        >
          <X :size="13" />
        </button>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <SegmentedControl
          v-model="outcome"
          :items="outcomeSegments"
          aria-label="执行结果状态切换"
          @update:model-value="emit('apply')"
        />

        <button
          type="button"
          class="inline-flex h-9 items-center gap-1.5 rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface)] px-3.5 text-xs font-medium text-[var(--ui-text-secondary)] transition hover:border-[var(--ui-line-strong)] hover:text-[var(--ui-text)]"
          :class="{ '!border-[var(--ui-accent)] !bg-[var(--ui-accent-soft)] !text-[var(--ui-accent)]': showAdvancedFilters }"
          @click="showAdvancedFilters = !showAdvancedFilters"
        >
          <Filter :size="13" />
          <span>高级过滤</span>
          <ChevronDown :size="12" class="transition-transform duration-200" :class="{ 'rotate-180': showAdvancedFilters }" />
        </button>

        <BaseButton variant="primary" size="sm" @click="emit('apply')">
          <Search :size="13" /> 检索
        </BaseButton>
      </div>
    </div>

    <div class="flex flex-wrap items-center gap-1.5 pt-0.5">
      <span class="mr-1 text-xs font-medium text-[var(--ui-text-muted)]">时间范围</span>
      <button
        v-for="preset in timePresets"
        :key="preset.value"
        type="button"
        class="rounded-full px-3 py-1 text-xs font-medium transition-colors"
        :class="
          timePreset === preset.value
            ? 'bg-[var(--ui-solid-accent)] text-[var(--ui-on-accent)] shadow-xs'
            : 'border border-[var(--ui-line)] bg-[var(--ui-surface)] text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)] hover:text-[var(--ui-text)]'
        "
        @click="emit('select-time-preset', preset.value)"
      >
        {{ preset.label }}
      </button>
    </div>

    <transition
      enter-active-class="transition-[opacity,transform] duration-200 ease-out"
      enter-from-class="opacity-0 -translate-y-1.5"
      enter-to-class="opacity-100 translate-y-0"
      leave-active-class="transition-[opacity,transform] duration-150 ease-in"
      leave-from-class="opacity-100 translate-y-0"
      leave-to-class="opacity-0 -translate-y-1.5"
    >
      <div v-if="showAdvancedFilters" class="grid gap-3 border-t border-[var(--ui-line)] pt-3 sm:grid-cols-2 lg:grid-cols-4">
        <div>
          <label for="audit-workspaceId" class="mb-1.5 block text-xs font-medium text-[var(--ui-text-muted)]">所属工作区</label>
          <div class="relative">
            <select
              id="audit-workspaceId"
              v-model="workspaceId"
              class="ui-select-native h-9 w-full cursor-pointer appearance-none truncate rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] pl-3 pr-8 text-xs text-[var(--ui-text)] outline-none transition focus:border-[var(--ui-accent)]"
              @change="emit('apply')"
            >
              <option value="">全部工作区与全局调用</option>
              <option v-for="ws in workspaces" :key="ws.id" :value="ws.id">{{ ws.name }}</option>
            </select>
            <ChevronDown :size="13" class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]" />
          </div>
        </div>

        <div>
          <label for="audit-toolName" class="mb-1.5 block text-xs font-medium text-[var(--ui-text-muted)]">工具名称精确匹配</label>
          <input
            id="audit-toolName"
            v-model="toolName"
            type="text"
            placeholder="如: apply_patch, exec_command"
            class="h-9 w-full rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] px-3 text-xs text-[var(--ui-text)] placeholder-[var(--ui-text-muted)] outline-none transition focus:border-[var(--ui-accent)]"
            @keydown.enter="emit('apply')"
          />
        </div>

        <div>
          <label for="audit-sessionId" class="mb-1.5 block text-xs font-medium text-[var(--ui-text-muted)]">Session ID 关联</label>
          <input
            id="audit-sessionId"
            v-model="sessionId"
            type="text"
            placeholder="匹配会话标识"
            class="h-9 w-full rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] px-3 text-xs text-[var(--ui-text)] placeholder-[var(--ui-text-muted)] outline-none transition focus:border-[var(--ui-accent)]"
            @keydown.enter="emit('apply')"
          />
        </div>

        <div v-if="timePreset === 'custom'" class="sm:col-span-2 lg:col-span-1">
          <label class="mb-1.5 block text-xs font-medium text-[var(--ui-text-muted)]">自定义起止时间</label>
          <div class="flex items-center gap-1.5">
            <input
              v-model="fromDate"
              aria-label="起始时间"
              type="datetime-local"
              class="h-9 w-full rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] px-2 text-xs text-[var(--ui-text)] outline-none focus:border-[var(--ui-accent)]"
              @change="emit('apply')"
            />
            <span class="text-xs text-[var(--ui-text-muted)]">至</span>
            <input
              v-model="toDate"
              aria-label="结束时间"
              type="datetime-local"
              class="h-9 w-full rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] px-2 text-xs text-[var(--ui-text)] outline-none focus:border-[var(--ui-accent)]"
              @change="emit('apply')"
            />
          </div>
        </div>
      </div>
    </transition>

    <div v-if="activeFilters.length > 0" class="flex flex-wrap items-center gap-1.5 pt-1">
      <span class="text-xs font-medium text-[var(--ui-text-muted)]">生效筛选:</span>
      <span
        v-for="filter in activeFilters"
        :key="filter.id"
        class="inline-flex items-center gap-1.5 rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] px-2.5 py-0.5 text-xs text-[var(--ui-text)]"
      >
        {{ filter.label }}
        <button type="button" class="text-[var(--ui-text-muted)] transition-colors hover:text-[var(--ui-danger)]" @click="filter.clear">
          <X :size="11" />
        </button>
      </span>

      <button type="button" class="ml-1 inline-flex items-center gap-1 text-xs font-medium text-[var(--ui-accent)] hover:underline" @click="emit('reset')">
        <RotateCcw :size="11" /> 清空所有筛选
      </button>
    </div>
  </div>
</template>
