<script setup lang="ts">
import { computed, ref } from "vue";
import { Check, Copy, Search } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import type { AgentContextSnapshotDto } from "$lib/api/agent-context";
import { useClipboard } from "$lib/composables/use-clipboard";

interface UnifiedScanItem {
  type: "instruction" | "skill";
  name: string;
  provider: string;
  path: string;
  description?: string;
  scope?: string;
}

const props = defineProps<{
  scanResult: AgentContextSnapshotDto;
}>();

const scanTab = ref<"all" | "instructions" | "skills">("all");
const scanSearch = ref("");
const { copiedKey, copyText } = useClipboard(1500);

const allScanItems = computed<UnifiedScanItem[]>(() => {
  const instructions: UnifiedScanItem[] = (props.scanResult.instructions ?? []).map((item) => ({
    type: "instruction",
    name: item.path.split("/").pop() || item.path,
    provider: item.provider,
    path: item.path,
    scope: item.scope,
  }));
  const skills: UnifiedScanItem[] = (props.scanResult.skills ?? []).map((skill) => ({
    type: "skill",
    name: skill.name,
    provider: skill.provider,
    path: skill.path,
    description: skill.description,
    scope: skill.scope,
  }));
  return [...instructions, ...skills];
});

const filteredScanItems = computed(() => {
  const q = scanSearch.value.trim().toLowerCase();
  return allScanItems.value.filter((item) => {
    if (scanTab.value === "instructions" && item.type !== "instruction") return false;
    if (scanTab.value === "skills" && item.type !== "skill") return false;
    if (!q) return true;
    return (
      item.name.toLowerCase().includes(q) ||
      item.provider.toLowerCase().includes(q) ||
      item.path.toLowerCase().includes(q) ||
      (item.description || "").toLowerCase().includes(q)
    );
  });
});

function providerBadgeClass(provider: string) {
  const p = (provider || "").toLowerCase();
  if (p === "codex") return "bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20";
  if (p === "cursor") return "bg-purple-500/10 text-purple-600 dark:text-purple-400 border border-purple-500/20";
  if (p === "claude") return "bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20";
  if (p === "opencode") return "bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20";
  if (p === "reasonix") return "bg-cyan-500/10 text-cyan-600 dark:text-cyan-400 border border-cyan-500/20";
  return "bg-[var(--ui-surface-subtle)] text-[var(--ui-text-secondary)] border border-[var(--ui-line)]";
}

async function copyItemPath(path: string) {
  await copyText(path, { key: path, label: "路径", silentSuccess: true });
}
</script>

<template>
  <div class="rounded-2xl border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] p-3">
    <div class="flex flex-wrap items-center justify-between gap-2.5 border-b border-[var(--ui-line)] pb-3">
      <div class="flex items-center gap-1">
        <button
          type="button"
          :class="scanTab === 'all' ? 'bg-[var(--ui-accent)] text-white shadow-xs' : 'text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)]'"
          class="cursor-pointer rounded-full px-2.5 py-1 text-xs font-medium transition"
          @click="scanTab = 'all'"
        >
          全部 ({{ allScanItems.length }})
        </button>
        <button
          type="button"
          :class="scanTab === 'instructions' ? 'bg-[var(--ui-accent)] text-white shadow-xs' : 'text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)]'"
          class="cursor-pointer rounded-full px-2.5 py-1 text-xs font-medium transition"
          @click="scanTab = 'instructions'"
        >
          指令 ({{ scanResult.instructions.length }})
        </button>
        <button
          type="button"
          :class="scanTab === 'skills' ? 'bg-[var(--ui-accent)] text-white shadow-xs' : 'text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)]'"
          class="cursor-pointer rounded-full px-2.5 py-1 text-xs font-medium transition"
          @click="scanTab = 'skills'"
        >
          技能 ({{ scanResult.skills.length }})
        </button>
      </div>

      <div class="relative min-w-[180px] max-w-xs flex-1">
        <Search :size="12" class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]" />
        <input
          v-model="scanSearch"
          type="text"
          placeholder="搜索技能名称或路径…"
          class="h-7 w-full rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] pl-7 pr-2.5 text-xs text-[var(--ui-text)] outline-none transition focus:border-[var(--ui-accent)]"
        />
      </div>
    </div>

    <div class="max-h-[320px] space-y-1.5 divide-y divide-[var(--ui-line)]/50 overflow-y-auto pt-2.5">
      <div
        v-for="item in filteredScanItems"
        :key="item.path"
        class="flex items-start justify-between gap-3 rounded-xl p-2 pb-1 pt-2 transition first:pt-0 hover:bg-[var(--ui-surface-hover)]"
      >
        <div class="min-w-0 flex-1 space-y-1">
          <div class="flex flex-wrap items-center gap-1.5">
            <span :class="providerBadgeClass(item.provider)" class="rounded-md px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wider">
              {{ item.provider }}
            </span>
            <span v-if="item.type === 'instruction'" class="rounded bg-sky-500/10 px-1.5 py-0.5 text-[10px] font-medium text-sky-600 dark:text-sky-400">
              指令规则
            </span>
            <strong class="truncate text-xs font-semibold text-[var(--ui-text)]">{{ item.name }}</strong>
          </div>
          <p v-if="item.description" class="line-clamp-2 text-[11px] leading-relaxed text-[var(--ui-text-secondary)]">
            {{ item.description }}
          </p>
          <code class="block max-w-full truncate rounded bg-[var(--ui-surface-subtle)] px-1.5 py-0.5 font-mono text-[10px] text-[var(--ui-text-muted)]" :title="item.path">
            {{ item.path }}
          </code>
        </div>
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
      <div v-if="!filteredScanItems.length" class="py-6 text-center text-xs text-[var(--ui-text-muted)]">
        未找到匹配的指令或技能
      </div>
    </div>
  </div>
</template>
