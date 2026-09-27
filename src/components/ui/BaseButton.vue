<script setup lang="ts">
import { computed } from "vue";
import { LoaderCircle } from "@lucide/vue";

const props = withDefaults(defineProps<{
  type?: "button" | "submit" | "reset";
  variant?: "primary" | "secondary" | "ghost" | "danger" | "icon";
  size?: "sm" | "md" | "lg";
  disabled?: boolean;
  busy?: boolean;
  title?: string;
}>(), {
  type: "button",
  variant: "primary",
  size: "md",
  disabled: false,
  busy: false,
});

defineEmits<{ click: [event: MouseEvent] }>();

const classes = computed(() => {
  const sizes = {
    sm: props.variant === "icon" ? "h-8 w-8 rounded-[var(--ui-radius-control)]" : "h-8 px-3 text-xs rounded-[var(--ui-radius-control)]",
    md: props.variant === "icon" ? "h-9 w-9 rounded-[var(--ui-radius-control)]" : "h-9 px-3.5 text-sm font-medium rounded-[var(--ui-radius-control)]",
    lg: props.variant === "icon" ? "h-11 w-11 rounded-[var(--ui-radius-control)]" : "h-11 px-4 text-sm font-medium rounded-[var(--ui-radius-control)]",
  };
  const variants = {
    primary: "border border-transparent bg-[var(--ui-accent)] text-white hover:bg-[var(--ui-accent-strong)]",
    secondary: "border border-[var(--ui-line)] bg-[var(--ui-surface)] text-[var(--ui-text)] backdrop-blur-xl hover:border-[var(--ui-line-strong)] hover:bg-[var(--ui-surface-raised)]",
    ghost: "border border-transparent bg-transparent text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)] hover:text-[var(--ui-text)]",
    danger: "border border-transparent bg-[var(--ui-danger)] text-white hover:brightness-105",
    icon: "border border-transparent bg-transparent text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)] hover:text-[var(--ui-text)]",
  };
  return [sizes[props.size], variants[props.variant]].join(" ");
});
</script>

<template>
  <button
    :type="type"
    :disabled="disabled || busy"
    :title="title"
    :class="classes"
    class="inline-flex shrink-0 cursor-pointer select-none items-center justify-center gap-2 tracking-tight outline-none transition-[background,border-color,color,opacity] duration-150 focus-visible:ring-2 focus-visible:ring-[var(--ui-accent)]/30 disabled:cursor-not-allowed disabled:opacity-40"
    @click="$emit('click', $event)"
  >
    <LoaderCircle v-if="busy" :size="size === 'sm' ? 13 : 16" class="animate-spin" />
    <slot />
  </button>
</template>
