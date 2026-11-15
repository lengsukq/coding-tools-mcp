<script setup lang="ts">
import { Activity, AlertCircle, CheckCircle2, Clock3, HardDrive, Layers, Settings2, Trash2 } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import ModalDialog from "$src/components/ui/ModalDialog.vue";

defineProps<{
  total: number;
  errorCount: number;
  workspaceCount: number;
  pageNumber: number;
  pageCount: number;
  successRate: number | null;
  selectedWorkspaceName: string;
  workspaceId: string;
  retentionDays: number;
  retentionPresets: readonly number[];
  savingRetention: boolean;
}>();

const retentionModalOpen = defineModel<boolean>("retentionModalOpen", { required: true });
const retentionInput = defineModel<number>("retentionInput", { required: true });

const emit = defineEmits<{
  "open-retention": [];
  "save-retention": [];
  "clear-records": [scope: "workspace" | "all"];
}>();
</script>

<template>
  <details class="audit-kpis">
    <summary>
      <span class="audit-kpis__title">审计概览</span>
      <span class="audit-kpis__hint">{{ total.toLocaleString() }} 条匹配记录 · 成功率 {{ successRate === null ? '暂无' : `${successRate}%` }} · 保留 {{ retentionDays }} 天</span>
    </summary>
    <div class="grid grid-cols-1 gap-3 pt-3 sm:grid-cols-2 lg:grid-cols-4 animate-fade-in-up delay-100">
      <GlassCard :padded="false" class="p-3 relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-[var(--text-muted)]">匹配调用总数</span>
          <div class="flex h-7 w-7 items-center justify-center rounded-lg bg-[var(--primary-soft)] text-[var(--primary)]">
            <Activity :size="15" />
          </div>
        </div>
        <div class="mt-3 flex items-baseline gap-2">
          <span class="text-xl sm:text-2xl font-bold tracking-tight text-[var(--text-main)] font-mono font-display">
            {{ total.toLocaleString() }}
          </span>
          <span class="text-xs text-[var(--text-muted)]">{{ total === 0 ? '暂无调用' : '次调用' }}</span>
        </div>
        <div class="mt-2 flex items-center justify-between text-xs text-[var(--text-muted)]">
          <span>当前筛选范围</span>
          <span class="font-medium text-[var(--text-secondary)]">{{ total === 0 ? '等待匹配记录' : `第 ${pageNumber} / ${pageCount} 页` }}</span>
        </div>
      </GlassCard>

      <GlassCard :padded="false" class="p-3 relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-[var(--text-muted)]">执行成功率</span>
          <div
            class="flex h-7 w-7 items-center justify-center rounded-lg"
            :class="total === 0 ? 'bg-[var(--ui-surface-subtle)] text-[var(--ui-text-muted)]' : errorCount === 0 ? 'bg-[var(--ui-success-soft)] text-[var(--ui-success)]' : 'bg-[var(--ui-warning-soft)] text-[var(--ui-warning)]'"
          >
            <Activity v-if="total === 0" :size="15" />
            <CheckCircle2 v-else-if="errorCount === 0" :size="15" />
            <AlertCircle v-else :size="15" />
          </div>
        </div>
        <div class="mt-3 flex items-baseline gap-2">
          <span
            class="text-xl sm:text-2xl font-bold tracking-tight font-mono font-display"
            :class="successRate === null ? 'text-[var(--ui-text-muted)]' : errorCount === 0 ? 'text-[var(--ui-success)]' : 'text-[var(--ui-warning)]'"
          >
            {{ successRate === null ? '—' : `${successRate}%` }}
          </span>
          <span class="text-xs text-[var(--text-muted)]">
            {{ successRate === null ? '暂无数据' : errorCount > 0 ? `${errorCount} 次失败` : '全部通过' }}
          </span>
        </div>
        <div v-if="successRate !== null" class="mt-2.5 h-1.5 w-full overflow-hidden rounded-full bg-[var(--ui-surface-subtle)]">
          <div
            class="h-full rounded-full transition-all duration-500 ease-out"
            :class="successRate >= 95 ? 'bg-[var(--ui-success)]' : successRate >= 80 ? 'bg-[var(--ui-warning)]' : 'bg-[var(--ui-danger)]'"
            :style="{ width: `${successRate}%` }"
          />
        </div>
      </GlassCard>

      <GlassCard :padded="false" class="p-3 relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-[var(--text-muted)]">覆盖工作区</span>
          <div class="flex h-7 w-7 items-center justify-center rounded-lg bg-[var(--primary-soft)] text-[var(--accent-indigo)]">
            <Layers :size="15" />
          </div>
        </div>
        <div class="mt-3 flex items-baseline gap-2">
          <span class="text-xl sm:text-2xl font-bold tracking-tight text-[var(--text-main)] font-mono font-display">
            {{ workspaceCount }}
          </span>
          <span class="text-xs text-[var(--text-muted)]">个活跃区域</span>
        </div>
        <p class="mt-2 truncate text-xs text-[var(--text-muted)]" :title="selectedWorkspaceName">
          当前焦点: <span class="font-medium text-[var(--text-secondary)]">{{ selectedWorkspaceName }}</span>
        </p>
      </GlassCard>

      <GlassCard :padded="false" class="p-3 relative overflow-hidden">
        <div class="flex items-center justify-between">
          <span class="text-xs font-medium text-[var(--text-muted)]">日志生命周期</span>
          <div class="flex h-7 w-7 items-center justify-center rounded-lg bg-[var(--ui-surface-subtle)] text-[var(--ui-text-secondary)]">
            <HardDrive :size="15" />
          </div>
        </div>
        <div class="mt-3 flex items-center justify-between">
          <div class="flex items-baseline gap-1.5">
            <span class="text-xl sm:text-2xl font-bold tracking-tight text-[var(--text-main)] font-mono font-display">
              {{ retentionDays }}
            </span>
            <span class="text-xs text-[var(--text-muted)]">天自动轮转</span>
          </div>
          <button
            type="button"
            class="flex items-center gap-1 rounded-md px-2 py-1 text-xs font-medium text-[var(--ui-accent)] transition hover:bg-[var(--ui-accent-soft)]"
            @click="emit('open-retention')"
          >
            <Settings2 :size="12" /> 配置
          </button>
        </div>
        <p class="mt-2 text-xs text-[var(--text-muted)]">
          本地存储 · 启动时安全清理过期项
        </p>
      </GlassCard>
    </div>
  </details>

  <details class="ui-disclosure">
    <summary>审计管理</summary>
    <div class="ui-disclosure-body">
      <p class="ui-muted">清理仅删除本地审计记录，不修改源码与计划；删除后无法恢复。</p>
      <div class="ui-actions">
        <BaseButton variant="ghost" size="sm" @click="emit('open-retention')">
          <Clock3 :size="13" /> 保留 {{ retentionDays }} 天
        </BaseButton>
        <BaseButton
          v-if="workspaceId"
          variant="ghost"
          size="sm"
          @click="emit('clear-records', 'workspace')"
        >
          <Trash2 :size="13" /> 清理当前工作区
        </BaseButton>
        <BaseButton variant="danger" size="sm" @click="emit('clear-records', 'all')">
          <Trash2 :size="13" /> 清空记录
        </BaseButton>
      </div>
    </div>
  </details>

  <ModalDialog
    :open="retentionModalOpen"
    title="配置审计日志保留期限"
    description="超过保留天数的历史工具调用记录将在应用启动或定时轮转中自动安全清理。"
    @close="retentionModalOpen = false"
  >
    <div class="space-y-4 pt-1">
      <div>
        <label class="block text-xs font-semibold text-[var(--text-main)] mb-2">快速预设周期</label>
        <div class="grid grid-cols-3 gap-2">
          <button
            v-for="days in retentionPresets"
            :key="days"
            type="button"
            class="rounded-xl border px-3 py-2 text-center transition-all"
            :class="retentionInput === days
              ? 'border-[var(--primary)] bg-[var(--primary-soft)] font-bold text-[var(--primary)] shadow-sm'
              : 'border-[var(--border)] bg-[var(--surface)] text-xs text-[var(--text-secondary)] hover:bg-[var(--surface-hover)]'"
            @click="retentionInput = days"
          >
            <span class="block text-sm font-semibold">{{ days }} 天</span>
            <span class="block text-xs text-[var(--text-muted)] mt-0.5">
              {{ days === 30 ? '默认推荐' : days === 365 ? '长期合规' : `${Math.round(days / 7)} 周` }}
            </span>
          </button>
        </div>
      </div>

      <div class="rounded-xl border border-[var(--border)] bg-[var(--surface-hover)] p-3">
        <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1.5">自定义保留天数 (1 ~ 365 天)</label>
        <div class="flex items-center gap-2">
          <input
            v-model.number="retentionInput"
            type="number"
            min="1"
            max="365"
            class="w-28 h-9 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-3 text-sm font-semibold font-mono text-[var(--text-main)] outline-none focus:border-[var(--primary)]"
          />
          <span class="text-xs text-[var(--text-muted)]">天</span>
        </div>
      </div>

      <div class="flex items-center justify-end gap-2 pt-2 border-t border-[var(--border-light)]">
        <BaseButton variant="ghost" size="sm" @click="retentionModalOpen = false">
          取消
        </BaseButton>
        <BaseButton variant="primary" size="sm" :busy="savingRetention" @click="emit('save-retention')">
          保存保留策略
        </BaseButton>
      </div>
    </div>
  </ModalDialog>
</template>
