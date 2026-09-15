<script setup lang="ts">
const model = defineModel<string>({ default: "" });
withDefaults(defineProps<{
  label?: string;
  placeholder?: string;
  type?: string;
  disabled?: boolean;
  hint?: string;
}>(), { type: "text", disabled: false });
</script>

<template>
  <label class="block min-w-0">
    <span v-if="label" class="mb-1.5 block text-xs font-semibold text-[var(--text-secondary)]">{{ label }}</span>
    <input
      v-model="model"
      :type="type"
      :placeholder="placeholder"
      :disabled="disabled"
      class="h-10 w-full rounded-xl border border-white/70 bg-white/64 px-3.5 text-sm text-[var(--text-main)] outline-none backdrop-blur-xl transition placeholder:text-[var(--text-muted)] focus:border-[#0a84ff]/50 focus:ring-4 focus:ring-[#0a84ff]/10 dark:border-white/10 dark:bg-white/6"
    />
    <span v-if="hint" class="mt-1.5 block text-[11px] leading-4 text-[var(--text-muted)]">{{ hint }}</span>
  </label>
</template>
