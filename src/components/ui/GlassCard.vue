<script setup lang="ts">
withDefaults(defineProps<{ padded?: boolean; interactive?: boolean }>(), {
  padded: true,
  interactive: false,
});
</script>

<template>
  <section
    class="ios-glass ios-card-surface"
    :class="[{ 'p-5': padded, 'ios-card-lift': interactive }]"
  >
    <slot />
  </section>
</template>
