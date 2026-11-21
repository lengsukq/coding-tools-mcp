<script setup lang="ts">
import { computed } from "vue";
import { LoaderCircle } from "@lucide/vue";

const props = withDefaults(
  defineProps<{
    type?: "button" | "submit" | "reset";
    variant?: "primary" | "secondary" | "ghost" | "danger";
    size?: "sm" | "md";
    pill?: boolean;
    disabled?: boolean;
    busy?: boolean;
    title?: string;
  }>(),
  {
    type: "button",
    variant: "primary",
    size: "md",
    pill: true,
    disabled: false,
    busy: false,
  },
);

defineEmits<{ click: [event: MouseEvent] }>();

const classes = computed(() => {
  const radius = props.pill ? "rounded-full" : "rounded-[var(--ui-radius-control)]";
  const sizes = {
    sm: `h-8 px-3.5 text-xs font-medium ${radius}`,
    md: `h-9 px-4 text-[13.5px] font-medium ${radius}`,
  };
  const variants = {
    primary:
      "border border-transparent bg-[var(--ui-solid-accent)] text-[var(--ui-on-accent)] shadow-[inset_0_1px_0_rgba(255,255,255,0.22),0_2px_8px_color-mix(in_srgb,var(--ui-accent)_24%,transparent)] hover:bg-[var(--ui-solid-accent-hover)]",
    secondary:
      "border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] text-[var(--ui-text)] shadow-[inset_0_1px_0_var(--ui-specular-top)] hover:border-[var(--ui-line-strong)] hover:bg-[var(--ui-surface-hover)]",
    ghost:
      "border border-transparent bg-transparent text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)] hover:text-[var(--ui-text)]",
    danger:
      "border border-transparent bg-[var(--ui-danger)] text-[var(--ui-on-danger)] shadow-[inset_0_1px_0_rgba(255,255,255,0.2)] hover:brightness-105",
  };
  return [sizes[props.size], variants[props.variant]].join(" ");
});
</script>

<template>
  <button
    :type="type"
    :disabled="disabled || busy"
    :aria-busy="busy"
    :title="title"
    :class="classes"
    class="inline-flex shrink-0 cursor-pointer select-none items-center justify-center gap-1.5 whitespace-nowrap tracking-tight outline-none transition-[background,border-color,color,opacity,transform,box-shadow] duration-150 active:scale-[0.985] focus-visible:ring-2 focus-visible:ring-[var(--ui-accent)]/35 disabled:pointer-events-none disabled:cursor-not-allowed disabled:opacity-45"
    @click="$emit('click', $event)"
  >
    <LoaderCircle v-if="busy" :size="size === 'sm' ? 13 : 15" class="animate-spin" />
    <slot />
  </button>
</template>
