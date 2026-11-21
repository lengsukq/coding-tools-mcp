<script setup lang="ts">
import { ref } from "vue";
import { Check, Copy, Eye, EyeOff } from "@lucide/vue";
import { useClipboard } from "$lib/composables/use-clipboard";

const model = defineModel<string>({ default: "" });
defineProps<{ disabled?: boolean }>();

const visible = ref(false);
const { copiedKey, copyText } = useClipboard(1500);

async function copy() {
  if (!model.value) return;
  await copyText(model.value, { key: "secret", label: "密钥", silentSuccess: true });
}
</script>

<template>
  <div class="flex items-center gap-2">
    <div class="relative min-w-0 flex-1">
      <input v-model="model" :disabled="disabled" :type="visible ? 'text' : 'password'" autocomplete="off" spellcheck="false" class="h-9 w-full rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-3 pr-20 font-mono text-xs text-[var(--ui-text)] outline-none transition-[background,border-color,box-shadow] duration-150 placeholder:text-[var(--ui-text-muted)] focus:border-[var(--ui-accent)] focus:bg-[var(--ui-surface-raised)] focus:shadow-[var(--focus-ring)]" />
      <div class="absolute inset-y-0 right-1.5 flex items-center gap-0.5">
        <button type="button" :aria-label="visible ? '隐藏密钥' : '显示密钥'" :title="visible ? '隐藏密钥' : '显示密钥'" class="flex h-7 w-7 items-center justify-center rounded-lg text-[var(--ui-text-secondary)] transition-colors hover:bg-[var(--ui-surface-hover)] focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--ui-accent)]" @click="visible = !visible"><EyeOff v-if="visible" :size="14" /><Eye v-else :size="14" /></button>
        <button type="button" :aria-label="copiedKey === 'secret' ? '密钥已复制' : '复制密钥'" :title="copiedKey === 'secret' ? '已复制' : '复制密钥'" class="flex h-7 w-7 items-center justify-center rounded-lg text-[var(--ui-text-secondary)] transition-colors hover:bg-[var(--ui-surface-hover)] focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-[var(--ui-accent)]" @click="copy"><Check v-if="copiedKey === 'secret'" :size="14" class="text-[var(--ui-success)]" /><Copy v-else :size="14" /></button>
      </div>
    </div>
  </div>
</template>
