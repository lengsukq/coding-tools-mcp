<script setup lang="ts">
import {useId} from "vue";
const fieldId=useId();
const model = defineModel<string>({ default: "" });
withDefaults(defineProps<{
  label?: string;
  placeholder?: string;
  type?: string;
  disabled?: boolean;
  hint?: string;
  error?: string;
}>(), { type: "text", disabled: false });
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
      class="h-9 w-full rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-3 text-sm text-[var(--ui-text)] backdrop-blur-lg outline-none transition-[background,border-color,box-shadow] duration-150 placeholder:text-[var(--ui-text-muted)] focus:border-[var(--ui-accent)] focus:bg-[var(--ui-surface-raised)] focus:shadow-[var(--focus-ring)] disabled:cursor-not-allowed disabled:opacity-40"
    />
    <span v-if="error" :id="fieldId" role="alert" class="mt-1.5 block text-xs text-[var(--ui-danger)]">{{error}}</span>
    <span v-else-if="hint" :id="fieldId" class="mt-1.5 block text-xs leading-4 text-[var(--ui-text-muted)]">{{ hint }}</span>
  </label>
</template>
