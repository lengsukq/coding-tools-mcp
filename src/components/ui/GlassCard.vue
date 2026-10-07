<script setup lang="ts">
withDefaults(defineProps<{ padded?: boolean; interactive?: boolean; hoverable?: boolean }>(), {
  padded: true,
  interactive: false,
  hoverable: false,
});
</script>

<template>
  <section
    class="ui-surface min-w-0 max-w-full"
    :class="[{ 'p-5': padded, 'ui-surface--interactive': interactive || hoverable }]"
  >
    <slot />
  </section>
</template>
