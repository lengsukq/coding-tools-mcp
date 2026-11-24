<script setup lang="ts">
import { Lock, Plus, Trash2 } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import type { KeyValueDraftRow } from "$lib/upstream-mcp/use-upstream-mcp";

const props = defineProps<{
  title: string;
  addLabel: string;
  emptyHint: string;
  keyPlaceholder: string;
  valuePlaceholder: string;
  deleteAriaLabel: string;
  secretTitleOn?: string;
  secretTitleOff?: string;
  rows: KeyValueDraftRow[];
  disabled?: boolean;
}>();

function addRow() {
  props.rows.push({ key: "", value: "", secret: false, hasStoredSecret: false });
}

function removeRow(index: number) {
  props.rows.splice(index, 1);
}
</script>

<template>
  <div class="space-y-2 rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] p-3">
    <div class="flex items-center justify-between gap-2">
      <span class="text-xs font-medium text-[var(--ui-text-secondary)]">{{ title }}</span>
      <BaseButton variant="secondary" size="sm" :disabled="disabled" @click="addRow">
        <Plus :size="12" />
        {{ addLabel }}
      </BaseButton>
    </div>
    <p v-if="!rows.length" class="text-xs text-[var(--ui-text-muted)]">
      {{ emptyHint }}
    </p>
    <div v-for="(row, idx) in rows" :key="`${title}-${idx}`" class="flex items-center gap-2">
      <input
        v-model="row.key"
        type="text"
        :placeholder="keyPlaceholder"
        :disabled="disabled"
        class="h-8 w-36 rounded-md border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] px-2 font-mono text-xs text-[var(--ui-text)] outline-none focus:border-[var(--ui-accent)]"
      />
      <input
        v-model="row.value"
        :type="row.secret ? 'password' : 'text'"
        :placeholder="row.hasStoredSecret ? '已加密保存（留空保持不变）' : valuePlaceholder"
        :disabled="disabled"
        class="h-8 min-w-0 flex-1 rounded-md border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] px-2 font-mono text-xs text-[var(--ui-text)] outline-none focus:border-[var(--ui-accent)]"
      />
      <button
        type="button"
        :disabled="disabled"
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md border transition"
        :class="
          row.secret
            ? 'border-[var(--ui-accent)] bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]'
            : 'border-[var(--ui-line)] text-[var(--ui-text-muted)] hover:text-[var(--ui-text)]'
        "
        :title="row.secret ? (secretTitleOn ?? '加密存储') : (secretTitleOff ?? '明文存储')"
        @click="row.secret = !row.secret"
      >
        <Lock :size="13" />
      </button>
      <button
        type="button"
        :disabled="disabled"
        class="flex h-8 w-8 shrink-0 items-center justify-center rounded-md text-[var(--ui-text-muted)] hover:text-[var(--ui-danger)]"
        :aria-label="deleteAriaLabel"
        @click="removeRow(idx)"
      >
        <Trash2 :size="13" />
      </button>
    </div>
  </div>
</template>
