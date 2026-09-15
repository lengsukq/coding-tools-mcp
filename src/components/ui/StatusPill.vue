<script setup lang="ts">
import { computed } from "vue";
const props = withDefaults(defineProps<{ status?: string; label?: string }>(), { status: "stopped" });
const tone = computed(() => {
  const value = props.status.toLowerCase();
  if (["running", "ok", "success", "active", "completed"].includes(value)) return "bg-[#30d158]/12 text-[#15913c] dark:text-[#5ee27a]";
  if (["starting", "stopping", "pending", "in_progress", "warning"].includes(value)) return "bg-[#ff9f0a]/14 text-[#b56b00] dark:text-[#ffb340]";
  if (["error", "failed", "blocked", "danger"].includes(value)) return "bg-[#ff375f]/12 text-[#d91e48] dark:text-[#ff6480]";
  return "bg-black/5 text-[var(--text-secondary)] dark:bg-white/8";
});
</script>

<template>
  <span :class="tone" class="inline-flex items-center gap-1.5 rounded-full px-2.5 py-1 text-[11px] font-semibold">
    <span class="h-1.5 w-1.5 rounded-full bg-current opacity-80" />
    {{ label || status }}
  </span>
</template>
