<script setup lang="ts">
import { Check, Copy } from "@lucide/vue";
import { ref } from "vue";
import { showToast } from "$lib/stores/toast";

const props = withDefaults(defineProps<{ label: string; value?: string; hint?: string; loading?: boolean }>(), {
  value: "",
  loading: false,
});

const copied = ref(false);

async function copy() {
  if (!props.value) return;
  try {
    await navigator.clipboard.writeText(props.value);
    copied.value = true;
    window.setTimeout(() => { copied.value = false; }, 1400);
  } catch {
    showToast("复制失败", { kind: "error" });
  }
}
</script>

<template>
  <div class="rounded-2xl border border-white/60 bg-white/40 p-3 dark:border-white/8 dark:bg-white/4">
    <div class="flex items-center justify-between gap-3">
      <div class="min-w-0 flex-1">
        <span class="block text-[10px] font-semibold uppercase tracking-[.08em] text-[var(--text-muted)]">{{ label }}</span>
        <code class="mt-1 block truncate font-mono text-[11px] text-[var(--text-main)]">{{ loading ? "加载中…" : value || "—" }}</code>
        <span v-if="hint" class="mt-1 block text-[10px] text-[var(--text-muted)]">{{ hint }}</span>
      </div>
      <button class="wb-icon-button !h-8 !w-8 !min-h-8" type="button" :disabled="!value || loading" @click="copy">
        <Check v-if="copied" :size="13" class="text-[var(--success)]" /><Copy v-else :size="13" />
      </button>
    </div>
  </div>
</template>
