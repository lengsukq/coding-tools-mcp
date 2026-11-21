<script setup lang="ts">
import { useId } from "vue";

const fieldId = useId();
const model = defineModel<string>({ default: "" });
withDefaults(
  defineProps<{
    label?: string;
    placeholder?: string;
    type?: string;
    disabled?: boolean;
    hint?: string;
    error?: string;
  }>(),
  { type: "text", disabled: false },
);
</script>

<template>
  <label class="block min-w-0">
    <span v-if="label" class="mb-1.5 block text-xs font-medium text-[var(--ui-text-secondary)]">{{ label }}</span>
    <input
      v-model="model"
      :type="type"
      :placeholder="placeholder"
      :disabled="disabled"
      :aria-invalid="!!error"
      :aria-describedby="error || hint ? fieldId : undefined"
      class="h-10 w-full rounded-[var(--ui-radius-control)] border bg-[var(--ui-surface-subtle)] px-3.5 text-sm text-[var(--ui-text)] outline-none transition-[background,border-color,box-shadow] duration-150 placeholder:text-[var(--ui-text-muted)] hover:border-[var(--ui-line-strong)] focus:border-[var(--ui-accent)] focus:bg-[var(--ui-surface-raised)] focus:shadow-[var(--focus-ring)] disabled:cursor-not-allowed disabled:opacity-45"
      :class="error ? 'border-[var(--ui-danger)]' : 'border-[var(--ui-line)]'"
    />
    <span v-if="error" :id="fieldId" role="alert" class="mt-1.5 block text-xs text-[var(--ui-danger)]">{{ error }}</span>
    <span v-else-if="hint" :id="fieldId" class="mt-1.5 block text-xs leading-relaxed text-[var(--ui-text-muted)]">{{ hint }}</span>
  </label>
</template>
