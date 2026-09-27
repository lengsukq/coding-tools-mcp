<script setup lang="ts">
defineProps<{ kicker?: string; title: string; description?: string }>();
</script>

<template>
  <header class="ui-page-header">
    <div class="ui-page-header__main">
      <p v-if="kicker" class="ui-page-header__kicker">{{ kicker }}</p>
      <h1 class="ui-page-header__title">{{ title }}</h1>
      <p v-if="description" class="ui-page-header__description">{{ description }}</p>
    </div>
    <div v-if="$slots.actions" class="ui-page-header__actions">
      <slot name="actions" />
    </div>
  </header>
</template>
