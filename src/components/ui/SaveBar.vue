<script setup lang="ts">
import BaseButton from "./BaseButton.vue";

withDefaults(
  defineProps<{
    dirty: boolean;
    busy?: boolean;
    error?: string;
    label?: string;
  }>(),
  { label: "保存更改" },
);
defineEmits<{ save: [] }>();
</script>

<template>
  <div class="ui-save-bar" :class="{ 'is-dirty': dirty && !error, 'is-error': !!error }" role="status">
    <span class="inline-flex min-w-0 items-center gap-2" :class="{ 'ui-danger font-medium': !!error }">
      <span
        class="h-2 w-2 shrink-0 rounded-full"
        :class="
          error
            ? 'bg-[var(--ui-danger)]'
            : dirty
              ? 'bg-[var(--ui-accent)] animate-pulse'
              : 'bg-[var(--ui-line-strong)]'
        "
        aria-hidden="true"
      />
      <span>{{ error || (busy ? "正在保存…" : dirty ? "有未保存的更改" : "所有更改已保存") }}</span>
    </span>
    <BaseButton size="sm" :busy="busy" :disabled="!dirty" @click="$emit('save')">{{ label }}</BaseButton>
  </div>
</template>
