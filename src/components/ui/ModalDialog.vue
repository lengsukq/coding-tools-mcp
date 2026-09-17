<script setup lang="ts">
import { nextTick, onBeforeUnmount, ref, useId, watch } from "vue";

const props = withDefaults(defineProps<{
  open: boolean;
  title?: string;
  description?: string;
  ariaLabel?: string;
  dismissible?: boolean;
}>(), { dismissible: true });
const emit = defineEmits<{ close: [] }>();
const dialogRef = ref<HTMLElement | null>(null);
const titleId = useId();
const descriptionId = useId();
let restoreFocus: HTMLElement | null = null;

function focusableElements() {
  return dialogRef.value?.querySelectorAll<HTMLElement>(
    'button:not([disabled]), [href], input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])',
  ) ?? [];
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

watch(() => props.open, async (open) => {
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
}, { immediate: true, flush: "post" });

onBeforeUnmount(() => {
  document.removeEventListener("keydown", handleKeydown);
  restoreFocus?.focus();
});
</script>

<template>
  <Teleport to="body">
    <Transition name="ios-modal">
      <div v-if="open" class="fixed inset-0 z-[100] flex items-center justify-center bg-black/20 p-6 backdrop-blur-md" @click.self="dismissible && emit('close')">
        <div
          ref="dialogRef"
          role="dialog"
          aria-modal="true"
          :aria-label="!title ? ariaLabel : undefined"
          :aria-labelledby="title ? titleId : undefined"
          :aria-describedby="description ? descriptionId : undefined"
          tabindex="-1"
          class="ios-glass-strong ios-floating-surface w-full max-w-lg p-5 outline-none"
        >
          <div v-if="title || description" class="mb-4">
            <h3 v-if="title" :id="titleId" class="text-lg font-semibold tracking-tight text-[var(--text-main)]">{{ title }}</h3>
            <p v-if="description" :id="descriptionId" class="mt-1 text-sm leading-6 text-[var(--text-secondary)]">{{ description }}</p>
          </div>
          <slot />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.ios-modal-enter-active,.ios-modal-leave-active{transition:opacity .18s ease}.ios-modal-enter-from,.ios-modal-leave-to{opacity:0}
</style>
