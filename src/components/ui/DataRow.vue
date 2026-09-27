<script setup lang="ts">
defineProps<{ interactive?: boolean }>();
</script>

<template>
  <div class="ui-data-row" :class="{ 'ui-surface--interactive': interactive }">
    <slot />
  </div>
</template>
