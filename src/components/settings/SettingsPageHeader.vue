<script setup lang="ts">
import PageHeader from "$src/components/ui/PageHeader.vue";
defineProps<{ kicker?: string; title: string; description?: string }>();
</script>

<template>
  <PageHeader :kicker="kicker || '全局设置'" :title="title" :description="description" />
</template>
