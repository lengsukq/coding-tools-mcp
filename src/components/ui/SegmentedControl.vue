<script setup lang="ts">
const props = withDefaults(defineProps<{
  items: Array<{ value: string; label: string }>;
  modelValue: string;
  ariaLabel?: string;
  disabled?: boolean;
}>(), { ariaLabel: "选项切换" });

const emit = defineEmits<{ "update:modelValue": [value: string] }>();

function handleKeydown(event: KeyboardEvent, index: number) {
  if(props.disabled) return;
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
  <div class="ui-options inline-flex max-w-full" role="radiogroup" aria-orientation="horizontal" :aria-label="ariaLabel">
    <button
      v-for="(item, index) in items"
      :key="item.value"
      :disabled="disabled"
      type="button"
      role="radio"
      :aria-checked="item.value === modelValue"
      :tabindex="item.value === modelValue ? 0 : -1"
      class="ui-options__item"
      :class="{ active: item.value === modelValue }"
      @click="emit('update:modelValue', item.value)"
      @keydown="handleKeydown($event, index)"
    >
      {{ item.label }}
    </button>
  </div>
</template>
