<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId, watch } from "vue";
import { X } from "@lucide/vue";

const props = withDefaults(
  defineProps<{
    open: boolean;
    title?: string;
    description?: string;
    ariaLabel?: string;
    dismissible?: boolean;
  }>(),
  { dismissible: true },
);
const emit = defineEmits<{ close: [] }>();
const dialogRef = ref<HTMLElement | null>(null);
const titleId = useId();
const descriptionId = useId();
let restoreFocus: HTMLElement | null = null;

function focusableElements() {
  return (
    dialogRef.value?.querySelectorAll<HTMLElement>(
      'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ) ?? []
  );
}

function handleKeydown(event: KeyboardEvent) {
  if (!props.open) return;
  if (event.key === "Escape" && props.dismissible) {
    event.preventDefault();
    emit("close");
    return;
  }
  if (event.key !== "Tab") return;
  const items = [...focusableElements()];
  if (items.length === 0) {
    event.preventDefault();
    dialogRef.value?.focus();
    return;
  }
  const first = items[0];
  const last = items[items.length - 1];
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}

watch(
  () => props.open,
  async (open) => {
    document.removeEventListener("keydown", handleKeydown);
    if (!open) {
      restoreFocus?.focus();
      restoreFocus = null;
      return;
    }
    restoreFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    document.addEventListener("keydown", handleKeydown);
    await nextTick();
    const first = focusableElements()[0];
    (first ?? dialogRef.value)?.focus();
  },
  { immediate: true, flush: "post" },
);

onBeforeUnmount(() => {
  document.removeEventListener("keydown", handleKeydown);
  restoreFocus?.focus();
});
</script>

<template>
  <Teleport to="body">
    <Transition name="ui-modal">
      <div
        v-if="open"
        class="fixed inset-0 z-[100] flex items-center justify-center bg-[var(--ui-overlay-backdrop)] p-6"
        @click.self="dismissible && emit('close')"
      >
        <div
          ref="dialogRef"
          role="dialog"
          aria-modal="true"
          :aria-label="!title ? ariaLabel : undefined"
          :aria-labelledby="title ? titleId : undefined"
          :aria-describedby="description ? descriptionId : undefined"
          tabindex="-1"
          class="ui-overlay w-full max-w-lg p-6 outline-none"
        >
          <div v-if="title || description" class="mb-5 flex items-start justify-between gap-4">
            <div class="min-w-0 flex-1">
              <h3 v-if="title" :id="titleId" class="text-lg font-semibold tracking-tight text-[var(--ui-text)]">{{ title }}</h3>
              <p v-if="description" :id="descriptionId" class="mt-1.5 text-sm leading-6 text-[var(--ui-text-secondary)]">{{ description }}</p>
            </div>
            <button
              v-if="dismissible"
              type="button"
              class="ui-icon-button -mr-1 -mt-1"
              aria-label="关闭对话框"
              @click="emit('close')"
            >
              <X :size="16" />
            </button>
          </div>
          <slot />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.ui-modal-enter-active,
.ui-modal-leave-active {
  transition: opacity 0.18s ease;
}
.ui-modal-enter-active .ui-overlay {
  transition:
    transform 0.22s var(--ease-spring),
    opacity 0.18s ease;
}
.ui-modal-leave-active .ui-overlay {
  transition:
    transform 0.14s ease-in,
    opacity 0.14s ease-in;
}
.ui-modal-enter-from,
.ui-modal-leave-to {
  opacity: 0;
}
.ui-modal-enter-from .ui-overlay {
  transform: scale(0.95) translateY(6px);
  opacity: 0;
}
.ui-modal-leave-to .ui-overlay {
  transform: scale(0.97) translateY(2px);
  opacity: 0;
}
</style>
