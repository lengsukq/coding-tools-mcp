<script setup lang="ts">
import {computed} from "vue";import {statusPresentation} from "$lib/ui-model";
const props=withDefaults(defineProps<{status?:string;label?:string}>(),{status:"unknown"});
const presentation=computed(()=>statusPresentation(props.status));
</script>
<template><span :class="`is-${presentation.tone}`" class="ui-status"><span class="ui-status-dot" aria-hidden="true" />{{label || presentation.label}}</span></template>
<style scoped>
.is-info { background: var(--ui-accent-soft); color: var(--ui-accent); border-color: var(--ui-line); }
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
