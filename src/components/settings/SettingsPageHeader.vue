<script setup lang="ts">
defineProps<{ kicker?: string; title: string; description?: string }>();
</script>

<template>
  <header class="mb-6">
    <p class="text-[10px] font-semibold uppercase tracking-[.16em] text-[#0a84ff]">{{ kicker || '全局设置' }}</p>
    <h1 class="mt-1.5 text-[26px] font-bold tracking-[-.035em] text-[var(--text-main)]">{{ title }}</h1>
    <p v-if="description" class="mt-1.5 max-w-3xl text-sm leading-6 text-[var(--text-secondary)]">{{ description }}</p>
  </header>
</template>
