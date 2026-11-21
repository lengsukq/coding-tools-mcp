<script setup lang="ts">
const model = defineModel<boolean>({ default: false });
defineProps<{ label?: string; description?: string; disabled?: boolean }>();
</script>

<template>
  <label class="flex cursor-pointer items-center justify-between gap-4 py-0.5" :class="disabled ? 'opacity-50' : ''">
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
      class="relative h-7 w-12 shrink-0 rounded-full border border-transparent transition duration-200 focus-visible:ring-2 focus-visible:ring-[var(--ui-accent)]/35"
      :class="model ? 'toggle-active' : 'bg-[var(--ui-line-strong)]'"
      @click.prevent="model = !model"
    >
      <span
        class="absolute left-0.5 top-0.5 h-5.5 w-5.5 rounded-full bg-[var(--ui-knob)] shadow-[0_2px_6px_rgba(0,0,0,0.18)] transition-transform duration-200 ease-[var(--ease-spring)]"
        :class="model ? 'translate-x-[20px]' : 'translate-x-0'"
      />
    </button>
  </label>
</template>

<style scoped>
.toggle-active {
  background: var(--ui-accent);
}
</style>
