<script setup lang="ts">
withDefaults(defineProps<{ padded?: boolean; interactive?: boolean }>(), {
  padded: true,
  interactive: false,
});
</script>

<template>
  <section
    class="ios-glass ios-card-surface min-w-0 max-w-full"
    :class="[{ 'p-5': padded, 'ios-card-lift': interactive }]"
  >
    <slot />
  </section>
</template>
