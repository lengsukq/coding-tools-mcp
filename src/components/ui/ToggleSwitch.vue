<script setup lang="ts">
const model = defineModel<boolean>({ default: false });
defineProps<{ label?: string; description?: string; disabled?: boolean }>();
</script>

<template>
  <label class="flex cursor-pointer items-center justify-between gap-4" :class="disabled ? 'opacity-50' : ''">
    <span v-if="label || description" class="min-w-0">
      <span v-if="label" class="block text-sm font-medium text-[var(--ui-text)]">{{ label }}</span>
      <span v-if="description" class="mt-0.5 block text-xs leading-5 text-[var(--ui-text-muted)]">{{ description }}</span>
    </span>
    <button
      type="button"
      role="switch"
      :aria-checked="model"
      :aria-label="label"
      :disabled="disabled"
      class="relative h-7 w-12 shrink-0 rounded-full transition duration-150 focus-visible:ring-2 focus-visible:ring-[var(--ui-accent)]/30"
      :class="model ? 'toggle-active' : 'bg-[var(--ui-line-strong)]'"
      @click.prevent="model = !model"
    >
      <span
        class="absolute top-0.5 h-6 w-6 rounded-full bg-[var(--ui-knob)] shadow-md transition duration-200"
        :class="model ? 'left-[22px]' : 'left-0.5'"
      />
    </button>
  </label>
</template>

<style scoped>
.toggle-active { background: var(--ui-accent); }
</style>
