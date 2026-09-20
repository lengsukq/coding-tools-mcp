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
.is-success {
  background: rgba(52, 199, 89, 0.12);
  color: #248a3d;
  border: 1px solid rgba(52, 199, 89, 0.22);
}
:global([data-theme="dark"]) .is-success,
:global(.dark) .is-success {
  background: rgba(48, 209, 88, 0.16);
  color: #30d158;
  border-color: rgba(48, 209, 88, 0.26);
}

.is-warning {
  background: rgba(255, 149, 0, 0.12);
  color: #c97000;
  border: 1px solid rgba(255, 149, 0, 0.22);
}
:global([data-theme="dark"]) .is-warning,
:global(.dark) .is-warning {
  background: rgba(255, 159, 10, 0.16);
  color: #ff9f0a;
  border-color: rgba(255, 159, 10, 0.26);
}

.is-danger {
  background: rgba(255, 59, 48, 0.12);
  color: #d70015;
  border: 1px solid rgba(255, 59, 48, 0.22);
}
:global([data-theme="dark"]) .is-danger,
:global(.dark) .is-danger {
  background: rgba(255, 69, 58, 0.16);
  color: #ff453a;
  border-color: rgba(255, 69, 58, 0.26);
}

.is-neutral {
  background: color-mix(in srgb, var(--text-main) 5%, transparent);
  color: var(--text-secondary);
  border: 1px solid color-mix(in srgb, var(--text-main) 8%, transparent);
}
</style>
