<script setup lang="ts">
withDefaults(
  defineProps<{
    padded?: boolean;
    variant?: "default" | "focal" | "subtle";
  }>(),
  {
    padded: true,
    variant: "default",
  },
);
</script>

<template>
  <section
    class="ui-surface min-w-0 max-w-full"
    :class="[
      {
        'p-6': padded,
        'ui-surface--focal': variant === 'focal',
        'ui-surface--subtle': variant === 'subtle',
      },
    ]"
  >
    <slot />
  </section>
</template>
