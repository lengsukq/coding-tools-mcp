<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, watch } from "vue";
import { CornerDownLeft, FolderGit2, Layers, Search, SlidersHorizontal, X } from "@lucide/vue";
import type { GlobalSearchEntry, SearchCategory } from "$lib/global-search";
import { searchShortcutLabel } from "$lib/platform";

const props = defineProps<{
  open: boolean;
  entries: GlobalSearchEntry[];
}>();

const query = defineModel<string>("query", { default: "" });

const emit = defineEmits<{
  close: [];
  select: [entry: GlobalSearchEntry];
}>();

const dialogRef = ref<HTMLElement | null>(null);
const inputRef = ref<HTMLInputElement | null>(null);
const activeIndex = ref(0);
let restoreFocus: HTMLElement | null = null;

function focusableElements(): HTMLElement[] {
  if (!dialogRef.value) return [];
  return [
    ...dialogRef.value.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ),
  ];
}

const groupedEntries = computed(() => {
  const order: SearchCategory[] = ["工作区", "设置页面", "页面与操作"];
  const groups: Array<{ category: SearchCategory; items: Array<{ entry: GlobalSearchEntry; flatIndex: number }> }> = [];
  let cursor = 0;
  for (const cat of order) {
    const matched = props.entries.filter((e) => e.category === cat);
    if (!matched.length) continue;
    groups.push({
      category: cat,
      items: matched.map((entry) => ({ entry, flatIndex: cursor++ })),
    });
  }
  return groups;
});

const flatList = computed(() => groupedEntries.value.flatMap((g) => g.items.map((i) => i.entry)));

watch(
  () => props.open,
  async (isOpen) => {
    if (!isOpen) {
      restoreFocus?.focus();
      restoreFocus = null;
      return;
    }
    restoreFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    activeIndex.value = 0;
    await nextTick();
    inputRef.value?.focus();
    inputRef.value?.select();
  },
);

onBeforeUnmount(() => {
  restoreFocus?.focus();
  restoreFocus = null;
});

watch(query, () => {
  activeIndex.value = 0;
});

function handleKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    emit("close");
    return;
  }
  if (event.key === "Tab") {
    const items = focusableElements();
    if (!items.length) {
      event.preventDefault();
      dialogRef.value?.focus();
      return;
    }
    const first = items[0];
    const last = items[items.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
    return;
  }
  if (!flatList.value.length) return;
  if (event.key === "ArrowDown") {
    event.preventDefault();
    activeIndex.value = (activeIndex.value + 1) % flatList.value.length;
  } else if (event.key === "ArrowUp") {
    event.preventDefault();
    activeIndex.value = (activeIndex.value - 1 + flatList.value.length) % flatList.value.length;
  } else if (event.key === "Enter") {
    event.preventDefault();
    const chosen = flatList.value[activeIndex.value];
    if (chosen) emit("select", chosen);
  }
}

function categoryIcon(category: SearchCategory) {
  if (category === "工作区") return FolderGit2;
  if (category === "设置页面") return SlidersHorizontal;
  return Layers;
}
</script>

<template>
  <Teleport to="body">
    <div
      v-if="open"
      class="fixed inset-0 z-50 flex items-start justify-center bg-[var(--ui-overlay-backdrop)] px-4 pt-[11vh]"
      @mousedown.self="emit('close')"
    >
      <div
        ref="dialogRef"
        role="dialog"
        aria-modal="true"
        aria-label="全局搜索与快捷跳转"
        tabindex="-1"
        class="ui-overlay w-full max-w-[640px] overflow-hidden outline-none"
        @keydown="handleKeydown"
      >
        <!-- Top Search Bar -->
        <div class="flex items-center gap-3 border-b border-[var(--ui-line)] px-4 py-3.5">
          <Search :size="18" class="shrink-0 text-[var(--ui-accent)]" />
          <input
            ref="inputRef"
            v-model="query"
            type="text"
            aria-label="全局搜索"
            placeholder="搜索工作区、设置项（如：端口、Gemini、局域网、白名单、代理）或页面…"
            class="h-7 min-w-0 flex-1 bg-transparent text-[14.5px] text-[var(--ui-text)] outline-none placeholder:text-[var(--ui-text-muted)]"
          />
          <button
            v-if="query"
            type="button"
            class="ui-icon-button h-6 w-6"
            aria-label="清空关键词"
            @click="query = ''; inputRef?.focus()"
          >
            <X :size="13" />
          </button>
          <kbd class="hidden rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-2 py-0.5 font-mono text-[11px] text-[var(--ui-text-muted)] sm:inline-block">
            ESC
          </kbd>
        </div>

        <!-- Grouped Results -->
        <div class="max-h-[58vh] overflow-y-auto p-2.5">
          <div v-if="!groupedEntries.length" class="py-12 text-center">
            <p class="text-sm font-medium text-[var(--ui-text-secondary)]">未找到与「{{ query }}」匹配的页面或设置</p>
            <p class="mt-1 text-xs text-[var(--ui-text-muted)]">试试搜索：端口、Gemini、局域网、密钥、代理、白名单、Computer Use 或工作区名称</p>
          </div>

          <div v-for="group in groupedEntries" :key="group.category" class="mb-3 last:mb-0">
            <div class="px-3 pb-1.5 pt-1 text-[11px] font-semibold tracking-wider text-[var(--ui-text-muted)]">
              {{ group.category }}
            </div>
            <div class="space-y-0.5">
              <button
                v-for="item in group.items"
                :key="item.entry.id"
                type="button"
                class="flex w-full items-center justify-between gap-3 rounded-[var(--ui-radius-control)] px-3 py-2.5 text-left transition duration-100"
                :class="
                  item.flatIndex === activeIndex
                    ? 'bg-[var(--ui-accent-soft)] text-[var(--ui-text)]'
                    : 'text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)] hover:text-[var(--ui-text)]'
                "
                @mouseenter="activeIndex = item.flatIndex"
                @click="emit('select', item.entry)"
              >
                <div class="flex min-w-0 flex-1 items-center gap-3">
                  <div
                    class="grid h-8 w-8 shrink-0 place-items-center rounded-[11px] border border-[var(--ui-line)]"
                    :class="
                      item.flatIndex === activeIndex
                        ? 'bg-[var(--ui-surface-raised)] text-[var(--ui-accent)]'
                        : 'bg-[var(--ui-surface-subtle)] text-[var(--ui-text-muted)]'
                    "
                  >
                    <component :is="categoryIcon(group.category)" :size="15" />
                  </div>
                  <div class="min-w-0 flex-1">
                    <div class="flex items-center gap-2">
                      <strong class="truncate text-[13.5px] font-semibold text-[var(--ui-text)]">
                        {{ item.entry.title }}
                      </strong>
                      <span
                        v-if="item.entry.badge"
                        class="shrink-0 rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-2 py-0.5 text-[10.5px] font-medium text-[var(--ui-text-muted)]"
                      >
                        {{ item.entry.badge }}
                      </span>
                    </div>
                    <p class="mt-0.5 truncate text-xs text-[var(--ui-text-muted)]">
                      {{ item.entry.subtitle }}
                    </p>
                  </div>
                </div>

                <span
                  class="inline-flex shrink-0 items-center gap-1 rounded-full px-2 py-0.5 text-[11px] font-medium"
                  :class="
                    item.flatIndex === activeIndex
                      ? 'bg-[var(--ui-surface-raised)] text-[var(--ui-accent)]'
                      : 'text-[var(--ui-text-muted)] opacity-60'
                  "
                >
                  <CornerDownLeft :size="11" /> 直达
                </span>
              </button>
            </div>
          </div>
        </div>

        <!-- Footer Shortcuts Hint -->
        <div class="flex items-center justify-between border-t border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-4 py-2 text-[11px] text-[var(--ui-text-muted)]">
          <div class="flex items-center gap-3">
            <span><kbd class="font-mono">↑↓</kbd> 选择</span>
            <span><kbd class="font-mono">↵</kbd> 直达页面</span>
            <span><kbd class="font-mono">ESC</kbd> 关闭</span>
          </div>
          <span>全站任意位置按 <kbd class="font-mono">{{ searchShortcutLabel }}</kbd> 唤起</span>
        </div>
      </div>
    </div>
  </Teleport>
</template>
