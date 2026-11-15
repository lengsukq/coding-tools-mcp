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
  <div class="border-b border-[var(--border-light)] bg-[var(--ui-surface-subtle)] p-4 lg:p-5 space-y-3.5">
    <div class="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
      <div class="relative flex-1 min-w-[240px] max-w-xl">
        <Search :size="14" class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
        <input
          v-model="query"
          type="text"
          aria-label="搜索审计记录"
          placeholder="搜索工具名称、Request ID、Session ID、客户端…"
          class="w-full h-9 rounded-xl border border-[var(--border)] bg-[var(--card-bg)] pl-9 pr-8 text-xs text-[var(--text-main)] placeholder-[var(--text-muted)] outline-none transition-all focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--primary)]/15"
          @keydown.enter="emit('apply')"
        />
        <button
          v-if="query"
          type="button"
          class="absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)] hover:text-[var(--text-main)]"
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
          class="inline-flex h-8 items-center gap-1.5 rounded-full border border-[var(--border)] px-3 text-xs font-medium text-[var(--text-secondary)] transition hover:bg-[var(--surface-hover)] hover:text-[var(--text-main)]"
          :class="{ 'border-[var(--primary)] text-[var(--primary)] bg-[var(--primary-soft)]': showAdvancedFilters }"
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
      <span class="text-xs font-medium text-[var(--text-muted)] mr-1">时间范围:</span>
      <button
        v-for="preset in timePresets"
        :key="preset.value"
        type="button"
        class="rounded-full px-3 py-1 text-xs font-medium transition-all"
        :class="timePreset === preset.value
          ? 'bg-[var(--ui-solid-accent)] text-[var(--ui-on-accent)] shadow-sm'
          : 'text-[var(--text-secondary)] border border-[var(--card-border)] hover:bg-[var(--primary-soft)] hover:text-[var(--text-main)]'"
        @click="emit('select-time-preset', preset.value)"
      >
        {{ preset.label }}
      </button>
    </div>

    <transition
      enter-active-class="transition duration-200 ease-out"
      enter-from-class="opacity-0 -translate-y-2 max-h-0"
      enter-to-class="opacity-100 translate-y-0 max-h-96"
      leave-active-class="transition duration-150 ease-in"
      leave-from-class="opacity-100 translate-y-0 max-h-96"
      leave-to-class="opacity-0 -translate-y-2 max-h-0"
    >
      <div v-if="showAdvancedFilters" class="grid gap-3 pt-2 border-t border-[var(--border-light)] sm:grid-cols-2 lg:grid-cols-4">
        <div>
          <label for="audit-workspaceId" class="block text-xs font-medium text-[var(--text-muted)] mb-1">所属工作区</label>
          <div class="relative">
            <select
              id="audit-workspaceId"
              v-model="workspaceId"
              class="w-full h-8 appearance-none rounded-lg border border-[var(--border)] bg-[var(--card-bg)] pl-3 pr-8 text-xs text-[var(--text-main)] outline-none transition-all focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--primary)]/15 cursor-pointer"
              @change="emit('apply')"
            >
              <option value="">全部工作区与全局调用</option>
              <option v-for="ws in workspaces" :key="ws.id" :value="ws.id">
                {{ ws.name }}
              </option>
            </select>
            <ChevronDown
              :size="13"
              class="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)]"
            />
          </div>
        </div>

        <div>
          <label for="audit-toolName" class="block text-xs font-medium text-[var(--text-muted)] mb-1">工具名称精确匹配</label>
          <input
            id="audit-toolName"
            v-model="toolName"
            type="text"
            placeholder="如: edit_file, execute_command"
            class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-3 text-xs text-[var(--text-main)] placeholder-[var(--text-muted)] outline-none transition-all focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--primary)]/15"
            @keydown.enter="emit('apply')"
          />
        </div>

        <div>
          <label for="audit-sessionId" class="block text-xs font-medium text-[var(--text-muted)] mb-1">Session ID 关联</label>
          <input
            id="audit-sessionId"
            v-model="sessionId"
            type="text"
            placeholder="匹配会话会签标识"
            class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-3 text-xs text-[var(--text-main)] placeholder-[var(--text-muted)] outline-none transition-all focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--primary)]/15"
            @keydown.enter="emit('apply')"
          />
        </div>

        <div v-if="timePreset === 'custom'" class="sm:col-span-2 lg:col-span-1">
          <label class="block text-xs font-medium text-[var(--text-muted)] mb-1">自定义起止时间</label>
          <div class="flex items-center gap-1.5">
            <input
              v-model="fromDate"
              aria-label="起始时间"
              type="datetime-local"
              class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-2 text-xs text-[var(--text-main)] outline-none focus:border-[var(--primary)]"
              @change="emit('apply')"
            />
            <span class="text-[var(--text-muted)] text-xs">至</span>
            <input
              v-model="toDate"
              aria-label="结束时间"
              type="datetime-local"
              class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-2 text-xs text-[var(--text-main)] outline-none focus:border-[var(--primary)]"
              @change="emit('apply')"
            />
          </div>
        </div>
      </div>
    </transition>

    <div v-if="activeFilters.length > 0" class="flex flex-wrap items-center gap-1.5 pt-1">
      <span class="text-xs font-medium text-[var(--text-muted)]">生效筛选:</span>
      <span
        v-for="filter in activeFilters"
        :key="filter.id"
        class="inline-flex items-center gap-1 rounded-md bg-[var(--surface-hover)] border border-[var(--border)] px-2 py-0.5 text-xs text-[var(--text-main)]"
      >
        {{ filter.label }}
        <button
          type="button"
          class="text-[var(--text-muted)] hover:text-[var(--ui-danger)] transition-colors"
          @click="filter.clear"
        >
          <X :size="11" />
        </button>
      </span>

      <button
        type="button"
        class="inline-flex items-center gap-1 text-xs text-[var(--primary)] hover:underline ml-1"
        @click="emit('reset')"
      >
        <RotateCcw :size="11" /> 清空所有筛选
      </button>
    </div>
  </div>
</template>
