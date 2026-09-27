<script setup lang="ts">
withDefaults(defineProps<{ reading?: boolean; paddedBottom?: boolean }>(), {
  reading: false,
  paddedBottom: true,
});
</script>

<template>
  <div class="ui-page" :class="[{ 'ui-page--reading': reading, 'pb-12': paddedBottom }]">
    <slot />
  </div>
</template>
