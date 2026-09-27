<script setup lang="ts">
defineProps<{ title: string; description?: string }>();
</script>

<template>
  <header class="ui-section-header">
    <div>
      <h2 class="ui-section-header__title">{{ title }}</h2>
      <p v-if="description" class="ui-section-header__description">{{ description }}</p>
    </div>
    <div v-if="$slots.actions"><slot name="actions" /></div>
  </header>
</template>
