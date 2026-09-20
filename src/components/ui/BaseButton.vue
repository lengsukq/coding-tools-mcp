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
    sm: props.variant === "icon" ? "h-8 w-8 rounded-lg" : "h-8 px-3 text-xs rounded-lg",
    md: props.variant === "icon" ? "h-9 w-9 rounded-xl" : "h-9 px-4 text-xs font-semibold rounded-xl",
    lg: props.variant === "icon" ? "h-11 w-11 rounded-2xl" : "h-11 px-5 text-sm font-semibold rounded-2xl",
  };
  const variants = {
    primary: "bg-[#0071e3] text-white shadow-[0_4px_14px_rgba(0,113,227,0.32),inset_0_1px_0_rgba(255,255,255,0.25)] hover:bg-[#0077ed] hover:brightness-105 hover:scale-[1.02] dark:bg-[#0a84ff] dark:hover:bg-[#198fff] dark:shadow-[0_4px_16px_rgba(10,132,255,0.36)]",
    secondary: "bg-[#E5E5EA] text-[#1d1d1f] shadow-[0_1px_2px_rgba(0,0,0,0.04)] hover:bg-[#dcdce0] hover:scale-[1.01] dark:bg-white/[0.09] dark:text-[#f5f5f7] dark:hover:bg-white/[0.14]",
    ghost: "bg-transparent text-[var(--text-secondary)] hover:bg-black/[0.05] hover:text-[var(--text-main)] dark:hover:bg-white/[0.08]",
    danger: "bg-[#ff3b30] text-white shadow-[0_4px_14px_rgba(255,59,48,0.28),inset_0_1px_0_rgba(255,255,255,0.2)] hover:bg-[#ff453a] hover:scale-[1.02]",
    icon: "bg-transparent text-[var(--text-secondary)] hover:bg-black/[0.05] hover:text-[var(--text-main)] hover:scale-[1.06] dark:hover:bg-white/[0.08]",
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
    class="inline-flex shrink-0 cursor-pointer select-none items-center justify-center gap-2 font-medium tracking-tight outline-none transition-all duration-200 ease-[cubic-bezier(0.25,1,0.5,1)] active:scale-[0.97] disabled:cursor-not-allowed disabled:opacity-40 disabled:hover:scale-100"
    @click="$emit('click', $event)"
  >
    <LoaderCircle v-if="busy" :size="size === 'sm' ? 13 : 16" class="animate-spin" />
    <slot />
  </button>
</template>
