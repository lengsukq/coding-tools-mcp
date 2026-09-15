<script setup lang="ts">
defineProps<{ open: boolean; title?: string; description?: string }>();
defineEmits<{ close: [] }>();
</script>

<template>
  <Teleport to="body">
    <Transition name="ios-modal">
      <div v-if="open" class="fixed inset-0 z-[100] flex items-center justify-center bg-black/20 p-6 backdrop-blur-md" @click.self="$emit('close')">
        <div class="ios-glass-strong ios-floating-surface w-full max-w-lg p-5">
          <div v-if="title || description" class="mb-4">
            <h3 v-if="title" class="text-lg font-semibold tracking-tight text-[var(--text-main)]">{{ title }}</h3>
            <p v-if="description" class="mt-1 text-sm leading-6 text-[var(--text-secondary)]">{{ description }}</p>
          </div>
          <slot />
        </div>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.ios-modal-enter-active,.ios-modal-leave-active{transition:opacity .18s ease}.ios-modal-enter-from,.ios-modal-leave-to{opacity:0}
</style>
