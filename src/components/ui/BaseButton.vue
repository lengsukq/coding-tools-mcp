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
    sm: props.variant === "icon" ? "h-8 w-8" : "h-8 px-3 text-xs",
    md: props.variant === "icon" ? "h-9 w-9" : "h-9 px-4 text-sm",
    lg: props.variant === "icon" ? "h-11 w-11" : "h-11 px-5 text-sm",
  };
  const variants = {
    primary: "bg-gradient-to-br from-[#0a84ff] via-[#4d6cff] to-[#7c5cff] text-white shadow-[0_8px_22px_rgba(10,132,255,.24)] hover:brightness-105",
    secondary: "ios-glass text-[var(--text-main)] hover:bg-white/80 dark:hover:bg-white/10",
    ghost: "text-[var(--text-secondary)] hover:bg-black/5 dark:hover:bg-white/8",
    danger: "bg-gradient-to-br from-[#ff375f] to-[#ff453a] text-white shadow-[0_8px_22px_rgba(255,55,95,.2)]",
    icon: "text-[var(--text-secondary)] hover:bg-black/5 dark:hover:bg-white/8",
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
    class="inline-flex shrink-0 cursor-pointer select-none items-center justify-center gap-2 rounded-xl font-medium transition duration-200 active:scale-[.97] disabled:cursor-not-allowed disabled:opacity-45"
    @click="$emit('click', $event)"
  >
    <LoaderCircle v-if="busy" :size="size === 'sm' ? 13 : 16" class="animate-spin" />
    <slot />
  </button>
</template>
