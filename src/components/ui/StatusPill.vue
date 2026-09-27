<script setup lang="ts">
import { computed } from "vue";
const props = withDefaults(defineProps<{ status?: string; label?: string }>(), { status: "stopped" });
const tone = computed(() => {
  const value = props.status.toLowerCase();
  if (["running", "ok", "success", "active", "completed"].includes(value)) return "is-success";
  if (["starting", "stopping", "pending", "in_progress", "warning"].includes(value)) return "is-warning";
  if (["error", "failed", "blocked", "danger"].includes(value)) return "is-danger";
  return "is-neutral";
});
</script>

<template>
  <span :class="tone" class="inline-flex items-center gap-1.5 rounded-md border px-2 py-1 text-xs font-medium">
    <span class="h-1.5 w-1.5 rounded-full bg-current opacity-80" />
    {{ label || status }}
  </span>
</template>

<style scoped>
.is-success {
  background: var(--ui-success-soft);
  color: var(--ui-success);
  border-color: color-mix(in srgb, var(--ui-success) 26%, transparent);
}

.is-warning {
  background: var(--ui-warning-soft);
  color: var(--ui-warning);
  border-color: color-mix(in srgb, var(--ui-warning) 26%, transparent);
}

.is-danger {
  background: var(--ui-danger-soft);
  color: var(--ui-danger);
  border-color: color-mix(in srgb, var(--ui-danger) 26%, transparent);
}

.is-neutral {
  background: var(--ui-surface-subtle);
  color: var(--ui-text-secondary);
  border-color: var(--ui-line);
}
</style>
