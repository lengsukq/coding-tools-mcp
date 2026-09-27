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
    <span v-if="label" class="mb-1.5 block text-xs font-medium text-[var(--ui-text-secondary)]">{{ label }}</span>
    <div class="relative">
      <select
        v-model="model"
        :disabled="disabled"
        class="h-10 w-full appearance-none rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-3 pr-9 text-sm text-[var(--ui-text)] outline-none transition-[background,border-color,box-shadow] duration-150 focus:border-[var(--ui-accent)] focus:bg-[var(--ui-surface-raised)] focus:shadow-[var(--focus-ring)] disabled:cursor-not-allowed disabled:opacity-40"
      >
        <option v-for="option in options" :key="option.value" :value="option.value">{{ option.label }}</option>
      </select>
      <span class="pointer-events-none absolute right-3 top-1/2 -translate-y-1/2 text-xs text-[var(--text-muted)] font-bold">⌄</span>
    </div>
    <span v-if="hint" class="mt-1.5 block text-xs leading-4 text-[var(--ui-text-muted)]">{{ hint }}</span>
  </label>
</template>
