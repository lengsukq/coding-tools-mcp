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
    <div
      class="fixed bottom-5 right-5 z-[1000] flex w-[368px] max-w-[calc(100vw-32px)] flex-col gap-2.5"
      aria-live="polite"
      aria-relevant="additions text"
    >
      <TransitionGroup name="toast">
        <div
          v-for="toast in toasts"
          :key="toast.id"
          :role="toast.kind === 'error' || toast.kind === 'warning' ? 'alert' : 'status'"
          class="ui-overlay flex items-start gap-3 rounded-[22px] px-4 py-3.5"
        >
          <div
            class="mt-0.5 grid h-7 w-7 shrink-0 place-items-center rounded-full"
            :class="{
              'bg-[var(--ui-success-soft)] text-[var(--ui-success)]': toast.kind === 'success',
              'bg-[var(--ui-warning-soft)] text-[var(--ui-warning)]': toast.kind === 'warning',
              'bg-[var(--ui-danger-soft)] text-[var(--ui-danger)]': toast.kind === 'error',
              'bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]': toast.kind === 'info',
            }"
          >
            <component :is="iconFor(toast.kind)" :size="15" />
          </div>
          <div class="min-w-0 flex-1">
            <strong v-if="toast.title" class="block text-xs font-semibold text-[var(--ui-text)]">{{ toast.title }}</strong>
            <p class="mt-0.5 text-xs leading-5 text-[var(--ui-text-secondary)]">{{ toast.message }}</p>
          </div>
          <button
            type="button"
            class="ui-icon-button -mr-1 -mt-0.5 h-7 w-7"
            :aria-label="`关闭${toast.title ? `：${toast.title}` : '通知'}`"
            @click="dismissToast(toast.id)"
          >
            <X :size="13" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>

<style scoped>
.toast-enter-active,
.toast-leave-active {
  transition:
    opacity 200ms var(--ease-spring),
    transform 200ms var(--ease-spring);
}
.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translateY(10px) scale(0.97);
}
</style>
