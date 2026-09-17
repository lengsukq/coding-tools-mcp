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
  <span :class="tone" class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-semibold">
    <span class="h-1.5 w-1.5 rounded-full bg-current opacity-80" />
    {{ label || status }}
  </span>
</template>

<style scoped>
.is-success { background: color-mix(in srgb, var(--success) 12%, transparent); color: var(--success); }
.is-warning { background: color-mix(in srgb, var(--warning) 13%, transparent); color: var(--warning); }
.is-danger { background: color-mix(in srgb, var(--danger) 12%, transparent); color: var(--danger); }
.is-neutral { background: color-mix(in srgb, var(--text-main) 5%, transparent); color: var(--text-secondary); }
</style>
