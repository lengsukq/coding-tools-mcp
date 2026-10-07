<script setup lang="ts">
import { useId } from "vue";
withDefaults(defineProps<{
    items: ReadonlyArray<{
        value: string;
        label: string;
    }>;
    modelValue: string;
    ariaLabel?: string;
}>(), { ariaLabel: "页面导航" });
const emit = defineEmits<{
    "update:modelValue": [
        value: string
    ];
}>();
const id = useId();
function move(event: KeyboardEvent, index: number, items: ReadonlyArray<{
    value: string;
}>) {
    let next = index;
    if (event.key === "ArrowRight")
        next = (index + 1) % items.length;
    else if (event.key === "ArrowLeft")
        next = (index - 1 + items.length) % items.length;
    else if (event.key === "Home")
        next = 0;
    else if (event.key === "End")
        next = items.length - 1;
    else
        return;
    event.preventDefault();
    (event.currentTarget as HTMLElement).parentElement?.querySelectorAll<HTMLButtonElement>('button')[next]?.focus();
    emit("update:modelValue", items[next].value);
}
</script>
<template>
 <div class="ui-tabs" role="tablist" :aria-label="ariaLabel">
  <button v-for="(item,index) in items" :id="`${id}-${item.value}`" :key="item.value" role="tab" type="button" :aria-selected="modelValue === item.value" :aria-controls="`${id}-panel`" :tabindex="modelValue === item.value ? 0 : -1" :class="{active:modelValue === item.value}" @click="emit('update:modelValue',item.value)" @keydown="move($event,index,items)">{{item.label}}</button>
 </div>
 <div :id="`${id}-panel`" role="tabpanel" :aria-labelledby="`${id}-${modelValue}`" tabindex="0" class="ui-tab-panel">
<slot />
</div>
</template>
