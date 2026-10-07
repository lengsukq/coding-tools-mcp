<script setup lang="ts">
defineProps<{
    title: string;
    description?: string;
}>();
</script>
<template><div class="ui-empty">
<h3>{{title}}</h3>
<p v-if="description">{{description}}</p>
<div v-if="$slots.default" class="ui-actions">
<slot />
</div>
</div></template>
