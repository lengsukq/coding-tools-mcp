<script setup lang="ts">
import { AlertTriangle, CheckCircle2, Info, X, XCircle } from "@lucide/vue";
import { dismissToast, toasts } from "$lib/stores/toast";

function iconFor(kind: string) {
  if (kind === "success") return CheckCircle2;
  if (kind === "warning") return AlertTriangle;
  if (kind === "error") return XCircle;
  return Info;
}
</script>

<template>
  <Teleport to="body">
    <div class="fixed bottom-5 right-5 z-[1000] flex w-[360px] max-w-[calc(100vw-40px)] flex-col gap-2" aria-live="polite" aria-relevant="additions text">
      <TransitionGroup name="toast">
        <div v-for="toast in toasts" :key="toast.id" :role="toast.kind === 'error' || toast.kind === 'warning' ? 'alert' : 'status'" class="tx-toast ios-glass-strong ios-card-surface flex items-start gap-3 p-3.5">
          <component
            :is="iconFor(toast.kind)"
            :size="16"
            class="mt-0.5 shrink-0"
            :class="{
              'text-[var(--success)]': toast.kind === 'success',
              'text-[var(--warning)]': toast.kind === 'warning',
              'text-[var(--danger)]': toast.kind === 'error',
              'text-[var(--primary)]': toast.kind === 'info',
            }"
          />
          <div class="min-w-0 flex-1">
            <strong v-if="toast.title" class="block text-xs text-[var(--text-main)]">{{ toast.title }}</strong>
            <p class="mt-0.5 text-[11px] leading-5 text-[var(--text-secondary)]">{{ toast.message }}</p>
            <button
              v-if="toast.action"
              type="button"
              class="mt-2 text-[11px] font-semibold text-[var(--primary)]"
              @click="toast.action.onClick()"
            >
              {{ toast.action.label }}
            </button>
          </div>
          <button type="button" class="wb-icon-button !h-6 !w-6 !min-h-6" :aria-label="`关闭${toast.title ? `：${toast.title}` : '通知'}`" @click="dismissToast(toast.id)">
            <X :size="12" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition: all 180ms ease;
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(8px) scale(0.98);
}
</style>
