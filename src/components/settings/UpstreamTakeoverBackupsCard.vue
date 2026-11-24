<script setup lang="ts">
import { History, RotateCcw } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import type { HarnessTakeoverBackupRecord } from "$lib/api/upstream-mcp";

defineProps<{
  backups: HarnessTakeoverBackupRecord[];
  restoringBackupId: string | null;
}>();

const emit = defineEmits<{
  restore: [record: HarnessTakeoverBackupRecord];
}>();

function formatTime(ms: number): string {
  try {
    return new Date(ms).toLocaleString();
  } catch {
    return String(ms);
  }
}
</script>

<template>
  <GlassCard v-if="backups.length" variant="subtle">
    <div class="space-y-3">
      <div class="flex items-center justify-between gap-2">
        <div class="flex items-center gap-2">
          <History :size="15" class="text-[var(--ui-accent)]" />
          <h3 class="text-sm font-semibold text-[var(--ui-text)]">已接管客户端配置备份</h3>
          <span class="rounded-full bg-[var(--ui-surface-raised)] px-2 py-0.5 font-mono text-[11px] text-[var(--ui-text-muted)]">
            {{ backups.length }}
          </span>
        </div>
        <span class="text-xs text-[var(--ui-text-muted)]">
          执行“聚合并替换”前自动保存原文件快照，随时可无损还原
        </span>
      </div>

      <div class="divide-y divide-[var(--ui-line)] rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-raised)]/50">
        <div
          v-for="item in backups"
          :key="item.id"
          class="flex flex-wrap items-center justify-between gap-3 px-3.5 py-2.5"
        >
          <div class="min-w-0 flex-1 space-y-0.5">
            <div class="flex flex-wrap items-center gap-2">
              <span class="text-xs font-semibold text-[var(--ui-text)]">{{ item.sourceLabel }}</span>
              <span class="font-mono text-[11px] text-[var(--ui-text-muted)]">{{ item.displayPath }}</span>
              <span class="rounded-full border border-[var(--ui-line)] px-1.5 py-0.5 text-[10px] text-[var(--ui-text-secondary)]">
                {{ item.connectMode === 'http' ? 'HTTP 直连' : 'mcp-stdio 桥接' }}
              </span>
              <span
                v-if="item.restoredAtMs"
                class="rounded-full bg-[var(--ui-success-soft)] px-1.5 py-0.5 text-[10px] text-[var(--ui-success)]"
              >
                已还原
              </span>
            </div>
            <p class="text-[11px] text-[var(--ui-text-secondary)]">
              原移除项：<code class="font-mono">{{ item.removedKeys.join(', ') }}</code>
              · 注入入口：<code class="font-mono">{{ item.injectedKey }}</code>
              · {{ formatTime(item.createdAtMs) }}
            </p>
          </div>

          <BaseButton
            variant="secondary"
            size="sm"
            :busy="restoringBackupId === item.id"
            :disabled="restoringBackupId !== null"
            @click="emit('restore', item)"
          >
            <RotateCcw v-if="restoringBackupId !== item.id" :size="12" />
            一键还原原配置
          </BaseButton>
        </div>
      </div>
    </div>
  </GlassCard>
</template>
