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
    sm: props.variant === "icon" ? "h-8 w-8 rounded-full" : "h-8 px-3.5 text-xs rounded-full",
    md: props.variant === "icon" ? "h-9 w-9 rounded-full" : "h-9 px-4.5 text-xs font-semibold rounded-full",
    lg: props.variant === "icon" ? "h-11 w-11 rounded-full" : "h-11 px-6 text-sm font-semibold rounded-full",
  };
  const variants = {
    primary: "bg-[image:var(--primary-gradient,var(--accent-gradient))] text-white shadow-[0_6px_20px_var(--coral-glow,rgba(0,113,227,0.32)),inset_0_1px_0_rgba(255,255,255,0.4)] hover:brightness-105 hover:scale-[1.02]",
    secondary: "bg-[var(--coral-blush)] text-[#111111] border border-[var(--card-border)] shadow-[0_1px_3px_rgba(var(--pal-rgb,0,113,227),0.08)] hover:bg-[var(--coral-soft)] hover:border-[var(--card-border-active)] hover:scale-[1.01] dark:bg-white/[0.08] dark:text-[#f5f5f7] dark:hover:bg-white/[0.12] dark:border-[var(--card-border)]",
    ghost: "bg-transparent text-[var(--text-secondary)] hover:bg-[var(--primary-soft)] hover:text-[var(--primary)] dark:hover:bg-white/[0.08]",
    danger: "bg-[var(--danger,#ff453a)] text-white shadow-[0_4px_14px_rgba(255,69,58,0.32),inset_0_1px_0_rgba(255,255,255,0.3)] hover:brightness-105 hover:scale-[1.02]",
    icon: "bg-transparent text-[var(--text-secondary)] hover:bg-[var(--primary-soft)] hover:text-[var(--primary)] hover:scale-[1.06] dark:hover:bg-white/[0.08]",
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
    class="btn-hover inline-flex shrink-0 cursor-pointer select-none items-center justify-center gap-2 font-medium tracking-tight outline-none transition-all duration-200 active:scale-[0.97] disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:scale-100"
    @click="$emit('click', $event)"
  >
    <LoaderCircle v-if="busy" :size="size === 'sm' ? 13 : 16" class="animate-spin" />
    <slot />
  </button>
</template>
