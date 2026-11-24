<script setup lang="ts">
withDefaults(defineProps<{ paddedBottom?: boolean }>(), {
  paddedBottom: true,
});
</script>

<template>
  <div class="ui-page" :class="[{ 'pb-12': paddedBottom }]">
    <slot />
  </div>
</template>
