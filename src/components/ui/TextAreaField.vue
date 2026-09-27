<script setup lang="ts">
const model = defineModel<string>({ default: "" });
withDefaults(defineProps<{ label?: string; placeholder?: string; hint?: string; mono?: boolean; rows?: number; disabled?: boolean }>(), {
  rows: 4,
  mono: false,
  disabled: false,
});
</script>

<template>
  <label class="block min-w-0">
    <span v-if="label" class="mb-1.5 block text-xs font-medium text-[var(--ui-text-secondary)]">{{ label }}</span>
    <textarea
      v-model="model"
      :rows="rows"
      :placeholder="placeholder"
      :disabled="disabled"
      class="w-full resize-y rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-3 py-2.5 text-sm leading-5 text-[var(--ui-text)] outline-none transition-[background,border-color,box-shadow] duration-150 placeholder:text-[var(--ui-text-muted)] focus:border-[var(--ui-accent)] focus:bg-[var(--ui-surface-raised)] focus:shadow-[var(--focus-ring)] disabled:cursor-not-allowed disabled:opacity-40"
      :class="mono ? 'font-mono text-xs' : ''"
    />
    <span v-if="hint" class="mt-1.5 block text-xs leading-4 text-[var(--ui-text-muted)]">{{ hint }}</span>
  </label>
</template>
