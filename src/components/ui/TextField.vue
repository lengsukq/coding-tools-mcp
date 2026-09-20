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
    <span v-if="label" class="mb-1.5 block text-xs font-semibold text-[var(--text-secondary)] tracking-tight">{{ label }}</span>
    <input
      v-model="model"
      :type="type"
      :placeholder="placeholder"
      :disabled="disabled"
      class="h-10 w-full rounded-xl border border-black/[0.04] bg-[#F2F2F7] px-3.5 text-sm text-[var(--text-main)] outline-none transition-all duration-200 ease-[cubic-bezier(0.25,1,0.5,1)] placeholder:text-[var(--text-muted)] focus:border-[#0071e3]/40 focus:bg-white focus:ring-4 focus:ring-[#0071e3]/15 dark:border-white/[0.07] dark:bg-white/[0.06] dark:focus:border-[#0a84ff]/50 dark:focus:bg-white/[0.11] dark:focus:ring-[#0a84ff]/20 disabled:cursor-not-allowed disabled:opacity-40"
    />
    <span v-if="hint" class="mt-1.5 block text-[11px] leading-4 text-[var(--text-muted)]">{{ hint }}</span>
  </label>
</template>
