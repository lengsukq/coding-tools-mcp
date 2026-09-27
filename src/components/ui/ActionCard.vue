<script setup lang="ts">
defineEmits<{ click: [event: MouseEvent] }>();
</script>

<template>
  <button type="button" class="ui-action-card" @click="$emit('click', $event)">
    <slot />
  </button>
</template>
