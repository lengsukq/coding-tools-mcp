<script setup lang="ts">
import { AlertTriangle } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import ModalDialog from "$src/components/ui/ModalDialog.vue";

withDefaults(defineProps<{
  open: boolean;
  title: string;
  message?: string;
  detail?: string;
  confirmText?: string;
  cancelText?: string;
  severity?: "warning" | "danger" | "info";
  busy?: boolean;
}>(), {
  confirmText: "确认",
  cancelText: "取消",
  severity: "warning",
  busy: false,
});

defineEmits<{ confirm: []; cancel: [] }>();
</script>

<template>
  <ModalDialog :open="open" :aria-label="title" :dismissible="!busy" @close="$emit('cancel')">
    <div class="flex items-start gap-3">
      <div
        class="grid h-10 w-10 shrink-0 place-items-center rounded-2xl"
        :class="
          severity === 'danger'
            ? 'bg-red-500/12 text-[var(--ui-danger)]'
            : severity === 'info'
              ? 'bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]'
              : 'bg-amber-500/12 text-[var(--ui-warning)]'
        "
      >
        <AlertTriangle :size="19" />
      </div>
      <div class="min-w-0 flex-1">
        <h3 class="text-base font-semibold text-[var(--ui-text)]">{{ title }}</h3>
        <p v-if="message" class="mt-1 text-sm leading-6 text-[var(--ui-text-secondary)]">{{ message }}</p>
        <p v-if="detail" class="mt-2 break-all rounded-xl bg-[var(--ui-surface-subtle)] px-3 py-2 font-mono text-xs text-[var(--ui-text-muted)]">{{ detail }}</p>
        <slot />
      </div>
    </div>
    <div class="mt-5 flex justify-end gap-2">
      <BaseButton variant="ghost" :disabled="busy" @click="$emit('cancel')">{{ cancelText }}</BaseButton>
      <BaseButton :variant="severity === 'danger' ? 'danger' : 'primary'" :busy="busy" @click="$emit('confirm')">{{ confirmText }}</BaseButton>
    </div>
  </ModalDialog>
</template>
