<script setup lang="ts">
withDefaults(defineProps<{ padded?: boolean; interactive?: boolean; hoverable?: boolean }>(), {
  padded: true,
  interactive: false,
  hoverable: false,
});
</script>

<template>
  <section
    class="ios-glass ios-card-surface min-w-0 max-w-full glass-card-refined"
    :class="[{ 'p-5': padded, 'ios-card-lift': interactive, 'card-hover': hoverable }]"
  >
    <slot />
  </section>
</template>

<style scoped>
.glass-card-refined {
  border-radius: var(--ios-radius-3xl, 26px);
  border: 1px solid var(--card-border, rgba(226, 87, 76, 0.14));
  background: var(--card-bg, rgba(255, 255, 255, 0.82));
  box-shadow: var(--card-shadow, inset 0 1px 0 rgba(255, 255, 255, 0.95), 0 16px 36px rgba(190, 120, 110, 0.12));
  backdrop-filter: blur(28px) saturate(180%);
  -webkit-backdrop-filter: blur(28px) saturate(180%);
  overflow: hidden;
  transition: transform 320ms var(--ease-dc-pop, cubic-bezier(0.34, 1.5, 0.5, 1)),
              box-shadow 320ms ease,
              border-color 280ms ease;
}

.glass-card-refined:hover {
  border-color: var(--card-border-active, rgba(var(--pal-rgb, 226, 87, 76), 0.32));
}

:global(.dark) .glass-card-refined,
:global([data-theme="dark"]) .glass-card-refined {
  border-color: var(--card-border, rgba(var(--pal-rgb, 253, 189, 180), 0.12));
  background: var(--card-bg, rgba(20, 20, 24, 0.72));
  box-shadow: var(--card-shadow, inset 0 1px 0 rgba(255, 255, 255, 0.12), 0 20px 50px rgba(0, 0, 0, 0.65));
}
</style>
