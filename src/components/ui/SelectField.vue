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
    <span v-if="label" class="mb-1.5 block text-xs font-semibold text-[var(--text-secondary)] tracking-tight">{{ label }}</span>
    <div class="relative">
      <select
        v-model="model"
        :disabled="disabled"
        class="h-10 w-full appearance-none rounded-xl border border-[var(--card-border)] bg-[var(--coral-50)]/80 px-3.5 pr-9 text-sm text-[var(--text-main)] outline-none transition-all duration-200 ease-[cubic-bezier(0.25,1,0.5,1)] focus:border-[var(--primary)]/50 focus:bg-white focus:ring-4 focus:ring-[var(--primary)]/15 dark:border-white/[0.07] dark:bg-white/[0.06] dark:focus:border-[var(--primary)]/60 dark:focus:bg-white/[0.11] dark:focus:ring-[var(--primary)]/20 disabled:cursor-not-allowed disabled:opacity-40"
      >
        <option v-for="option in options" :key="option.value" :value="option.value">{{ option.label }}</option>
      </select>
      <span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-[var(--text-muted)] font-bold">⌄</span>
    </div>
    <span v-if="hint" class="mt-1.5 block text-[11px] leading-4 text-[var(--text-muted)]">{{ hint }}</span>
  </label>
</template>
