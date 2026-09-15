<script setup lang="ts">
defineProps<{
  items: Array<{ value: string; label: string }>;
  modelValue: string;
}>();

defineEmits<{ "update:modelValue": [value: string] }>();
</script>

<template>
  <div class="tx-segmented ios-glass ios-inset-surface inline-flex max-w-full gap-1 p-1">
    <button
      v-for="item in items"
      :key="item.value"
      type="button"
      class="tx-tab rounded-xl px-3 py-1.5 text-[11px] font-medium transition"
      :class="{ active: item.value === modelValue }"
      @click="$emit('update:modelValue', item.value)"
    >
      {{ item.label }}
    </button>
  </div>
</template>
