<script setup lang="ts">
const model = defineModel<boolean>({ default: false });
defineProps<{ label?: string; description?: string; disabled?: boolean }>();
</script>

<template>
  <label class="flex cursor-pointer items-center justify-between gap-4 rounded-2xl px-1 py-1" :class="disabled ? 'opacity-50' : ''">
    <span v-if="label || description" class="min-w-0">
      <span v-if="label" class="block text-sm font-medium text-[var(--text-main)]">{{ label }}</span>
      <span v-if="description" class="mt-0.5 block text-xs leading-5 text-[var(--text-muted)]">{{ description }}</span>
    </span>
    <button
      type="button"
      role="switch"
      :aria-checked="model"
      :disabled="disabled"
      class="relative h-7 w-12 shrink-0 rounded-full transition duration-200"
      :class="model ? 'toggle-active' : 'bg-black/15 dark:bg-white/16'"
      @click.prevent="model = !model"
    >
      <span
        class="absolute top-0.5 h-6 w-6 rounded-full bg-white shadow-md transition duration-200"
        :class="model ? 'left-[22px]' : 'left-0.5'"
      />
    </button>
  </label>
</template>

<style scoped>
.toggle-active { background: var(--success); }
</style>
