<script setup lang="ts">
const model = defineModel<string>({ default: "" });
defineProps<{
  label?: string;
  options: Array<{ value: string; label: string }> | ReadonlyArray<{ value: string; label: string }>;
  disabled?: boolean;
  hint?: string;
}>();
</script>

<template>
  <label class="block min-w-0">
    <span v-if="label" class="mb-1.5 block text-xs font-semibold text-[var(--text-secondary)]">{{ label }}</span>
    <div class="relative">
      <select
        v-model="model"
        :disabled="disabled"
        class="h-10 w-full appearance-none rounded-xl border border-white/70 bg-white/64 px-3.5 pr-9 text-sm text-[var(--text-main)] outline-none backdrop-blur-xl transition focus:border-[#0a84ff]/50 focus:ring-4 focus:ring-[#0a84ff]/10 dark:border-white/10 dark:bg-white/6"
      >
        <option v-for="option in options" :key="option.value" :value="option.value">{{ option.label }}</option>
      </select>
      <span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-[10px] text-[var(--text-muted)]">⌄</span>
    </div>
    <span v-if="hint" class="mt-1.5 block text-[11px] leading-4 text-[var(--text-muted)]">{{ hint }}</span>
  </label>
</template>
