<script setup lang="ts">
defineProps<{
  items: Array<{ value: string; label: string }>;
  modelValue: string;
}>();

defineEmits<{ "update:modelValue": [value: string] }>();
</script>

<template>
  <div class="ios-segmented inline-flex max-w-full">
    <button
      v-for="item in items"
      :key="item.value"
      type="button"
      class="ios-segmented__item"
      :class="{ active: item.value === modelValue }"
      @click="$emit('update:modelValue', item.value)"
    >
      {{ item.label }}
    </button>
  </div>
</template>

<style scoped>
.ios-segmented {
  align-items: center;
  gap: 2px;
  padding: 3px;
  overflow-x: auto;
  border: 1px solid rgba(15, 23, 42, .045);
  border-radius: 12px;
  background: rgba(118, 118, 128, .095);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, .46);
  scrollbar-width: none;
}

.ios-segmented::-webkit-scrollbar { display: none; }

.ios-segmented__item {
  min-height: 30px;
  padding: 5px 12px;
  border: 0;
  border-radius: 9px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 11.5px;
  font-weight: 560;
  line-height: 1;
  white-space: nowrap;
  cursor: pointer;
  transition: background 150ms ease, color 150ms ease, box-shadow 150ms ease, transform 150ms ease;
}

.ios-segmented__item:hover:not(.active) {
  background: rgba(255, 255, 255, .34);
  color: var(--text-main);
}

.ios-segmented__item:active { transform: scale(.985); }

.ios-segmented__item.active {
  background: rgba(255, 255, 255, .88);
  color: var(--text-main);
  font-weight: 650;
  box-shadow: 0 1px 4px rgba(15, 23, 42, .09), inset 0 1px 0 rgba(255, 255, 255, .82);
}

:global(.dark) .ios-segmented {
  border-color: rgba(255, 255, 255, .055);
  background: rgba(118, 118, 128, .16);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, .035);
}

:global(.dark) .ios-segmented__item:hover:not(.active) { background: rgba(255, 255, 255, .055); }
:global(.dark) .ios-segmented__item.active {
  background: rgba(255, 255, 255, .13);
  box-shadow: 0 1px 5px rgba(0, 0, 0, .2), inset 0 1px 0 rgba(255, 255, 255, .045);
}
</style>
