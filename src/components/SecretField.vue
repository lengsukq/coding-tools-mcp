<script setup lang="ts">
import { computed, ref } from "vue";
import { Check, Copy, Eye, EyeOff, RotateCw } from "@lucide/vue";
import BaseButton from "./ui/BaseButton.vue";

const model = defineModel<string>({ default: "" });
const props = withDefaults(defineProps<{ value?: string; busy?: boolean; allowRegenerate?: boolean; readonly?: boolean }>(), {
  value: undefined,
  busy: false,
  allowRegenerate: false,
  readonly: false,
});
defineEmits<{ regenerate: [] }>();
const visible = ref(false);
const copied = ref(false);
const fieldValue = computed({
  get: () => props.value ?? model.value,
  set: (value: string) => {
    if (props.value === undefined) model.value = value;
  },
});

async function copy() {
  if (!fieldValue.value) return;
  await navigator.clipboard.writeText(fieldValue.value);
  copied.value = true;
  setTimeout(() => { copied.value = false; }, 1500);
}
</script>

<template>
  <div class="flex items-center gap-2">
    <div class="relative min-w-0 flex-1">
      <input v-model="fieldValue" :readonly="readonly" :type="visible ? 'text' : 'password'" class="h-10 w-full rounded-xl border border-white/70 bg-white/60 px-3.5 pr-20 font-mono text-xs outline-none dark:border-white/10 dark:bg-white/6" />
      <div class="absolute inset-y-0 right-1.5 flex items-center gap-0.5">
        <button type="button" class="flex h-7 w-7 items-center justify-center rounded-lg text-[var(--text-muted)] hover:bg-black/5 dark:hover:bg-white/8" @click="visible = !visible"><EyeOff v-if="visible" :size="14" /><Eye v-else :size="14" /></button>
        <button type="button" class="flex h-7 w-7 items-center justify-center rounded-lg text-[var(--text-muted)] hover:bg-black/5 dark:hover:bg-white/8" @click="copy"><Check v-if="copied" :size="14" class="text-[#30d158]" /><Copy v-else :size="14" /></button>
      </div>
    </div>
    <BaseButton v-if="allowRegenerate" variant="secondary" size="sm" :busy="busy" @click="$emit('regenerate')"><RotateCw :size="13" />重新生成</BaseButton>
  </div>
</template>
