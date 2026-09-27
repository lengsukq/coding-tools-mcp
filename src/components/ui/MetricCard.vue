<script setup lang="ts">
defineProps<{ label: string; value: string | number; detail?: string }>();
</script>

<template>
  <div class="ui-metric">
    <div class="flex items-center justify-between gap-3">
      <span class="ui-metric__label">{{ label }}</span>
      <slot name="icon" />
    </div>
    <strong class="ui-metric__value"><slot>{{ value }}</slot></strong>
    <span v-if="detail || $slots.detail" class="ui-metric__detail"><slot name="detail">{{ detail }}</slot></span>
  </div>
</template>
