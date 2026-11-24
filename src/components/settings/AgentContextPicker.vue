<script setup lang="ts">
import { computed, ref } from "vue";
import { Check, Copy, RefreshCw, Search, X } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import type { InstructionDocumentDto, SkillDescriptorDto } from "$lib/api/agent-context";
import { setBatchDisabledIds, toggleDisabledId } from "$lib/agent-context";
import { useClipboard } from "$lib/composables/use-clipboard";

interface PickerItem {
  id: string;
  type: "skill" | "instruction";
  name: string;
  description: string;
  path: string;
  scope: string;
  enabled: boolean;
  globallyDisabled: boolean;
}

const props = withDefaults(
  defineProps<{
    instructions: InstructionDocumentDto[];
    skills: SkillDescriptorDto[];
    disabledInstructionIds: string[];
    disabledSkillIds: string[];
    globallyDisabledInstructionIds?: string[];
    globallyDisabledSkillIds?: string[];
    scanning?: boolean;
    hasScanned?: boolean;
    emptyHint?: string;
  }>(),
  {
    globallyDisabledInstructionIds: () => [],
    globallyDisabledSkillIds: () => [],
    scanning: false,
    hasScanned: true,
    emptyHint: "尚未扫描到本机 Skills 或 Agents 指令，可点击右上角重新扫描或在高级选项中添加自定义路径。",
  },
);

const emit = defineEmits<{
  "update:disabledInstructionIds": [ids: string[]];
  "update:disabledSkillIds": [ids: string[]];
  rescan: [];
}>();

const activeTab = ref<"all" | "skills" | "instructions">("all");
const searchQuery = ref("");
const { copiedKey, copyText } = useClipboard(1500);

const disabledSkillSet = computed(() => new Set(props.disabledSkillIds));
const disabledInstructionSet = computed(() => new Set(props.disabledInstructionIds));
const globalDisabledSkillSet = computed(() => new Set(props.globallyDisabledSkillIds));
const globalDisabledInstructionSet = computed(() => new Set(props.globallyDisabledInstructionIds));

function instructionPreview(content: string): string {
  const firstLine = content
    .split(/\r?\n/)
    .map((line) => line.replace(/^#+\s*/, "").trim())
    .find((line) => line.length > 0 && !line.startsWith("<!--"));
  return firstLine ?? "";
}

function scopeLabel(scope: string): string {
  if (scope === "workspace") return "工作区";
  if (scope === "custom") return "自定义";
  return "全局";
}

const allItems = computed<PickerItem[]>(() => {
  const skillItems: PickerItem[] = props.skills.map((skill) => {
    const globallyDisabled = globalDisabledSkillSet.value.has(skill.id);
    return {
      id: skill.id,
      type: "skill",
      name: skill.name,
      description: skill.description,
      path: skill.path,
      scope: skill.scope,
      globallyDisabled,
      enabled: !globallyDisabled && !disabledSkillSet.value.has(skill.id),
    };
  });
  const instructionItems: PickerItem[] = props.instructions.map((doc) => {
    const globallyDisabled = globalDisabledInstructionSet.value.has(doc.id);
    const filename = doc.path.split(/[\\/]/).pop() || doc.path;
    return {
      id: doc.id,
      type: "instruction",
      name: filename,
      description: instructionPreview(doc.content),
      path: doc.path,
      scope: doc.scope,
      globallyDisabled,
      enabled: !globallyDisabled && !disabledInstructionSet.value.has(doc.id),
    };
  });
  return [...skillItems, ...instructionItems];
});

const filteredItems = computed(() => {
  const q = searchQuery.value.trim().toLowerCase();
  return allItems.value.filter((item) => {
    if (activeTab.value === "skills" && item.type !== "skill") return false;
    if (activeTab.value === "instructions" && item.type !== "instruction") return false;
    if (!q) return true;
    return (
      item.name.toLowerCase().includes(q) ||
      item.path.toLowerCase().includes(q) ||
      item.description.toLowerCase().includes(q)
    );
  });
});

const enabledCount = computed(() => allItems.value.filter((item) => item.enabled).length);
const selectableFilteredItems = computed(() =>
  filteredItems.value.filter((item) => !item.globallyDisabled),
);

function toggleItem(item: PickerItem, checked: boolean) {
  if (item.globallyDisabled) return;
  if (item.type === "skill") {
    emit("update:disabledSkillIds", toggleDisabledId(props.disabledSkillIds, item.id, checked));
  } else {
    emit(
      "update:disabledInstructionIds",
      toggleDisabledId(props.disabledInstructionIds, item.id, checked),
    );
  }
}

function setAllVisible(enabled: boolean) {
  const visibleSkillIds = selectableFilteredItems.value
    .filter((item) => item.type === "skill")
    .map((item) => item.id);
  const visibleInstructionIds = selectableFilteredItems.value
    .filter((item) => item.type === "instruction")
    .map((item) => item.id);

  if (visibleSkillIds.length > 0) {
    emit(
      "update:disabledSkillIds",
      setBatchDisabledIds(props.disabledSkillIds, visibleSkillIds, enabled),
    );
  }
  if (visibleInstructionIds.length > 0) {
    emit(
      "update:disabledInstructionIds",
      setBatchDisabledIds(props.disabledInstructionIds, visibleInstructionIds, enabled),
    );
  }
}

async function copyItemPath(path: string) {
  await copyText(path, { key: path, label: "路径", silentSuccess: true });
}
</script>

<template>
  <div class="rounded-2xl border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] p-3.5">
    <!-- Top Summary & Rescan -->
    <div class="flex flex-wrap items-center justify-between gap-2.5 border-b border-[var(--ui-line)] pb-3">
      <div class="min-w-0">
        <p class="text-xs font-semibold text-[var(--ui-text)]">
          已发现 {{ skills.length }} 个 Skills · {{ instructions.length }} 个 Agents 指令
        </p>
        <p class="mt-0.5 text-[11px] text-[var(--ui-text-muted)]">
          勾选即允许注入到 MCP 会话，取消勾选则排除；当前已启用 {{ enabledCount }} / {{ allItems.length }} 项。
        </p>
      </div>
      <div class="flex flex-wrap items-center gap-1.5">
        <BaseButton
          v-if="selectableFilteredItems.length"
          variant="ghost"
          size="sm"
          @click="setAllVisible(true)"
        >
          全选
        </BaseButton>
        <BaseButton
          v-if="selectableFilteredItems.length"
          variant="ghost"
          size="sm"
          @click="setAllVisible(false)"
        >
          全部取消
        </BaseButton>
        <BaseButton variant="secondary" size="sm" :busy="scanning" @click="emit('rescan')">
          <RefreshCw :size="12" :class="scanning ? 'animate-spin' : ''" />
          <span>重新扫描</span>
        </BaseButton>
      </div>
    </div>

    <!-- Filter Tabs & Search -->
    <div class="mt-3 flex flex-wrap items-center justify-between gap-2.5">
      <div class="flex items-center gap-1">
        <button
          type="button"
          :class="activeTab === 'all' ? 'bg-[var(--ui-accent)] text-white shadow-xs' : 'text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)]'"
          class="cursor-pointer rounded-full px-2.5 py-1 text-xs font-medium transition"
          @click="activeTab = 'all'"
        >
          全部 ({{ allItems.length }})
        </button>
        <button
          type="button"
          :class="activeTab === 'skills' ? 'bg-[var(--ui-accent)] text-white shadow-xs' : 'text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)]'"
          class="cursor-pointer rounded-full px-2.5 py-1 text-xs font-medium transition"
          @click="activeTab = 'skills'"
        >
          Skills ({{ skills.length }})
        </button>
        <button
          type="button"
          :class="activeTab === 'instructions' ? 'bg-[var(--ui-accent)] text-white shadow-xs' : 'text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)]'"
          class="cursor-pointer rounded-full px-2.5 py-1 text-xs font-medium transition"
          @click="activeTab = 'instructions'"
        >
          Agents 指令 ({{ instructions.length }})
        </button>
      </div>

      <div class="relative min-w-[180px] max-w-xs flex-1">
        <Search
          :size="12"
          class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]"
        />
        <input
          v-model="searchQuery"
          type="text"
          placeholder="搜索技能名称、说明或路径…"
          class="h-7.5 w-full rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] pl-7 pr-7 text-xs text-[var(--ui-text)] outline-none transition focus:border-[var(--ui-accent)]"
        />
        <button
          v-if="searchQuery"
          type="button"
          class="absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)] hover:text-[var(--ui-text)]"
          aria-label="清空搜索"
          @click="searchQuery = ''"
        >
          <X :size="12" />
        </button>
      </div>
    </div>

    <!-- List Body -->
    <div v-if="scanning && !allItems.length" class="py-8 text-center text-xs text-[var(--ui-text-muted)]">
      正在扫描本机 Skills 与 Agents 指令…
    </div>
    <div
      v-else-if="!hasScanned || !allItems.length"
      class="mt-3 rounded-xl border border-dashed border-[var(--ui-line)] px-4 py-6 text-center text-xs text-[var(--ui-text-muted)]"
    >
      {{ emptyHint }}
    </div>
    <div
      v-else-if="!filteredItems.length"
      class="py-6 text-center text-xs text-[var(--ui-text-muted)]"
    >
      没有匹配「{{ searchQuery }}」的 Skills 或 Agents 指令
    </div>
    <div
      v-else
      class="mt-3 max-h-[360px] divide-y divide-[var(--ui-line)]/50 overflow-y-auto pr-1"
    >
      <div
        v-for="item in filteredItems"
        :key="`${item.type}:${item.id}`"
        class="flex items-start gap-3 py-2.5 transition first:pt-1 last:pb-1"
        :class="item.enabled ? '' : 'opacity-65'"
      >
        <input
          :id="`ctx-item-${item.type}-${item.id}`"
          type="checkbox"
          :checked="item.enabled"
          :disabled="item.globallyDisabled"
          class="mt-1 h-4 w-4 shrink-0 cursor-pointer rounded accent-[var(--ui-accent)] disabled:cursor-not-allowed"
          @change="toggleItem(item, ($event.target as HTMLInputElement).checked)"
        />
        <label
          :for="`ctx-item-${item.type}-${item.id}`"
          class="min-w-0 flex-1 space-y-1"
          :class="item.globallyDisabled ? 'cursor-not-allowed' : 'cursor-pointer'"
        >
          <div class="flex flex-wrap items-center gap-1.5">
            <strong class="truncate text-xs font-semibold text-[var(--ui-text)]">{{ item.name }}</strong>
            <span
              class="rounded-md px-1.5 py-0.5 text-[10px] font-medium"
              :class="
                item.type === 'skill'
                  ? 'border border-emerald-500/20 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300'
                  : 'border border-sky-500/20 bg-sky-500/10 text-sky-700 dark:text-sky-300'
              "
            >
              {{ item.type === "skill" ? "Skill" : "Agents" }}
            </span>
            <span
              class="rounded-md border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-1.5 py-0.5 text-[10px] text-[var(--ui-text-muted)]"
            >
              {{ scopeLabel(item.scope) }}
            </span>
            <span
              v-if="item.globallyDisabled"
              class="rounded-md border border-amber-500/25 bg-amber-500/10 px-1.5 py-0.5 text-[10px] font-medium text-amber-700 dark:text-amber-300"
            >
              已在全局禁用
            </span>
            <span
              v-else-if="!item.enabled"
              class="rounded-md border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-1.5 py-0.5 text-[10px] text-[var(--ui-text-muted)]"
            >
              不注入
            </span>
          </div>
          <p
            v-if="item.description"
            class="line-clamp-2 text-[11px] leading-relaxed text-[var(--ui-text-secondary)]"
          >
            {{ item.description }}
          </p>
          <code
            class="block max-w-full truncate rounded bg-[var(--ui-surface-subtle)] px-1.5 py-0.5 font-mono text-[10px] text-[var(--ui-text-muted)]"
            :title="item.path"
          >
            {{ item.path }}
          </code>
        </label>
        <BaseButton
          variant="ghost"
          size="sm"
          class="h-7 w-7 shrink-0 !p-0"
          :title="copiedKey === item.path ? '已复制' : '复制完整路径'"
          @click="copyItemPath(item.path)"
        >
          <Check v-if="copiedKey === item.path" :size="12" class="text-[var(--ui-success)]" />
          <Copy v-else :size="12" class="text-[var(--ui-text-muted)]" />
        </BaseButton>
      </div>
    </div>
  </div>
</template>
