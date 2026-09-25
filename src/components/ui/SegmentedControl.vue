<script setup lang="ts">
const props = withDefaults(defineProps<{
  items: Array<{ value: string; label: string }>;
  modelValue: string;
  ariaLabel?: string;
}>(), { ariaLabel: "选项切换" });

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

function handleKeydown(event: KeyboardEvent, index: number) {
  let target = index;
  if (["ArrowRight", "ArrowDown"].includes(event.key)) target = (index + 1) % props.items.length;
  else if (["ArrowLeft", "ArrowUp"].includes(event.key)) target = (index - 1 + props.items.length) % props.items.length;
  else if (event.key === "Home") target = 0;
  else if (event.key === "End") target = props.items.length - 1;
  else return;
  event.preventDefault();
  emit("update:modelValue", props.items[target].value);
  const buttons = (event.currentTarget as HTMLElement).parentElement?.querySelectorAll<HTMLButtonElement>('[role="radio"]');
  requestAnimationFrame(() => buttons?.[target]?.focus());
}
</script>

<template>
  <div class="ios-segmented inline-flex max-w-full" role="radiogroup" aria-orientation="horizontal" :aria-label="ariaLabel">
    <button
      v-for="(item, index) in items"
      :key="item.value"
      type="button"
      role="radio"
      :aria-checked="item.value === modelValue"
      :tabindex="item.value === modelValue ? 0 : -1"
      class="ios-segmented__item"
      :class="{ active: item.value === modelValue }"
      @click="emit('update:modelValue', item.value)"
      @keydown="handleKeydown($event, index)"
    >
      {{ item.label }}
    </button>
  </div>
</template>

<style scoped>
.ios-segmented {
  align-items: center;
  gap: 4px;
  padding: 4px;
  overflow-x: auto;
  border: 1px solid var(--card-border, rgba(var(--pal-rgb, 226, 87, 76), 0.16));
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.65);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.8), 0 2px 8px rgba(var(--pal-rgb, 190, 120, 110), 0.06);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  scrollbar-width: none;
}

.ios-segmented::-webkit-scrollbar { display: none; }

.ios-segmented__item {
  min-height: 32px;
  padding: 6px 16px;
  border: 1px solid transparent;
  border-radius: 999px;
  background: transparent;
  color: var(--text-secondary);
  font-size: 12px;
  font-weight: 560;
  line-height: 1;
  white-space: nowrap;
  cursor: pointer;
  transition: background 180ms ease, color 180ms ease, box-shadow 180ms ease, transform 180ms ease;
}

.ios-segmented__item:hover:not(.active) {
  background: var(--primary-soft, rgba(var(--pal-rgb, 226, 87, 76), 0.08));
  color: var(--text-main);
}

.ios-segmented__item:active { transform: scale(.96); }

.ios-segmented__item.active {
  background: var(--primary-gradient, var(--primary));
  color: #ffffff;
  font-weight: 650;
  box-shadow: 0 4px 14px var(--coral-glow, rgba(var(--pal-rgb, 0, 113, 227), 0.28)), inset 0 1px 0 rgba(255, 255, 255, 0.35);
}

:global(.dark) .ios-segmented,
:global([data-theme="dark"]) .ios-segmented {
  border-color: var(--card-border, rgba(var(--pal-rgb, 253, 189, 180), 0.14));
  background: rgba(20, 20, 24, 0.65);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.06);
}

:global(.dark) .ios-segmented__item:hover:not(.active),
:global([data-theme="dark"]) .ios-segmented__item:hover:not(.active) {
  background: rgba(255, 255, 255, 0.08);
}
:global(.dark) .ios-segmented__item.active,
:global([data-theme="dark"]) .ios-segmented__item.active {
  background: var(--primary-gradient, var(--primary));
  color: #ffffff;
  box-shadow: 0 4px 14px var(--coral-glow, rgba(var(--pal-rgb, 0, 113, 227), 0.32)), inset 0 1px 0 rgba(255, 255, 255, 0.3);
}
</style>
