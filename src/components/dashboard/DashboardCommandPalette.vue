<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { GitBranch, Search, Settings2, Zap } from "@lucide/vue";

interface CommandEntryView {
  label: string;
  hint: string;
}

const props = defineProps<{
  open: boolean;
  entries: CommandEntryView[];
}>();
const query = defineModel<string>("query", { default: "" });
const emit = defineEmits<{ close: []; select: [index: number] }>();
const inputRef = ref<HTMLInputElement | null>(null);
const paletteRef = ref<HTMLElement | null>(null);

watch(() => props.open, async (open) => {
  if (!open) return;
  await nextTick();
  inputRef.value?.focus();
});

function trapTab(event: KeyboardEvent) {
  if (event.key !== "Tab") return;
  const focusable = paletteRef.value?.querySelectorAll<HTMLElement>('input, button:not([disabled]), [tabindex]:not([tabindex="-1"])');
  if (!focusable?.length) return;
  const items = [...focusable];
  const first = items[0];
  const last = items[items.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
</script>

<template>
  <Teleport to="body">
    <div v-if="open" class="wb-command-backdrop" @click.self="emit('close')">
      <div ref="paletteRef" class="wb-command-palette ios-glass-strong rounded-[24px]" role="dialog" aria-modal="true" aria-label="Quick Actions" @keydown="trapTab">
        <div class="wb-command-search">
          <Search :size="15" />
          <input
            ref="inputRef"
            v-model="query"
            placeholder="搜索工作区或操作…"
            @keydown.enter="entries[0] && emit('select', 0)"
            @keydown.esc="emit('close')"
          />
          <span class="wb-kbd">ESC</span>
        </div>
        <div class="wb-command-results">
          <div v-if="entries.length === 0" class="wb-empty-inline">没有匹配的操作。</div>
          <button
            v-for="(entry, index) in entries"
            v-else
            :key="entry.label"
            class="wb-command-item"
            :class="{ active: index === 0 }"
            type="button"
            @click="emit('select', index)"
          >
            <Settings2 v-if="entry.hint === 'Settings'" :size="14" />
            <Zap v-else-if="entry.hint === 'Runtime'" :size="14" />
            <GitBranch v-else :size="14" />
            <span>{{ entry.label }}</span><small>{{ entry.hint }}</small>
          </button>
        </div>
      </div>
    </div>
  </Teleport>
</template>
