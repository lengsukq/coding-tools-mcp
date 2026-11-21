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
  <div class="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-4">
    <GlassCard :padded="false" class="p-4">
      <div class="flex items-center justify-between">
        <span class="text-xs font-medium text-[var(--ui-text-muted)]">匹配调用总数</span>
        <div class="grid h-8 w-8 place-items-center rounded-full bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]">
          <Activity :size="15" />
        </div>
      </div>
      <div class="mt-3 flex items-baseline gap-2">
        <span class="font-mono text-2xl font-bold tracking-tight tabular-nums text-[var(--ui-text)]">
          {{ total.toLocaleString() }}
        </span>
        <span class="text-xs text-[var(--ui-text-muted)]">{{ total === 0 ? "暂无调用" : "次调用" }}</span>
      </div>
      <div class="mt-2 flex items-center justify-between text-xs text-[var(--ui-text-muted)]">
        <span>当前页码</span>
        <span class="font-mono font-medium text-[var(--ui-text-secondary)]">{{ total === 0 ? "—" : `${pageNumber} / ${pageCount}` }}</span>
      </div>
    </GlassCard>

    <GlassCard :padded="false" class="p-4">
      <div class="flex items-center justify-between">
        <span class="text-xs font-medium text-[var(--ui-text-muted)]">执行成功率</span>
        <div
          class="grid h-8 w-8 place-items-center rounded-full"
          :class="
            total === 0
              ? 'bg-[var(--ui-surface-subtle)] text-[var(--ui-text-muted)]'
              : errorCount === 0
                ? 'bg-[var(--ui-success-soft)] text-[var(--ui-success)]'
                : 'bg-[var(--ui-warning-soft)] text-[var(--ui-warning)]'
          "
        >
          <Activity v-if="total === 0" :size="15" />
          <CheckCircle2 v-else-if="errorCount === 0" :size="15" />
          <AlertCircle v-else :size="15" />
        </div>
      </div>
      <div class="mt-3 flex items-baseline gap-2">
        <span
          class="font-mono text-2xl font-bold tracking-tight tabular-nums"
          :class="successRate === null ? 'text-[var(--ui-text-muted)]' : errorCount === 0 ? 'text-[var(--ui-success)]' : 'text-[var(--ui-warning)]'"
        >
          {{ successRate === null ? "—" : `${successRate}%` }}
        </span>
        <span class="text-xs text-[var(--ui-text-muted)]">
          {{ successRate === null ? "暂无数据" : errorCount > 0 ? `${errorCount} 次失败` : "全部通过" }}
        </span>
      </div>
      <div v-if="successRate !== null" class="mt-2.5 h-1.5 w-full overflow-hidden rounded-full bg-[var(--ui-line)]">
        <div
          class="h-full rounded-full transition-[width] duration-500 ease-out"
          :class="successRate >= 95 ? 'bg-[var(--ui-success)]' : successRate >= 80 ? 'bg-[var(--ui-warning)]' : 'bg-[var(--ui-danger)]'"
          :style="{ width: `${successRate}%` }"
        />
      </div>
    </GlassCard>

    <GlassCard :padded="false" class="p-4">
      <div class="flex items-center justify-between">
        <span class="text-xs font-medium text-[var(--ui-text-muted)]">覆盖工作区</span>
        <div class="grid h-8 w-8 place-items-center rounded-full bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]">
          <Layers :size="15" />
        </div>
      </div>
      <div class="mt-3 flex items-baseline gap-2">
        <span class="font-mono text-2xl font-bold tracking-tight tabular-nums text-[var(--ui-text)]">
          {{ workspaceCount }}
        </span>
        <span class="text-xs text-[var(--ui-text-muted)]">个活跃工作区</span>
      </div>
      <p class="mt-2 truncate text-xs text-[var(--ui-text-muted)]" :title="selectedWorkspaceName">
        筛选范围: <span class="font-medium text-[var(--ui-text-secondary)]">{{ selectedWorkspaceName }}</span>
      </p>
    </GlassCard>

    <GlassCard :padded="false" class="p-4">
      <div class="flex items-center justify-between">
        <span class="text-xs font-medium text-[var(--ui-text-muted)]">日志保留周期</span>
        <div class="grid h-8 w-8 place-items-center rounded-full bg-[var(--ui-surface-subtle)] text-[var(--ui-text-secondary)]">
          <HardDrive :size="15" />
        </div>
      </div>
      <div class="mt-3 flex items-center justify-between">
        <div class="flex items-baseline gap-1.5">
          <span class="font-mono text-2xl font-bold tracking-tight tabular-nums text-[var(--ui-text)]">
            {{ retentionDays }}
          </span>
          <span class="text-xs text-[var(--ui-text-muted)]">天自动轮转</span>
        </div>
        <button
          type="button"
          class="inline-flex items-center gap-1 rounded-full bg-[var(--ui-accent-soft)] px-2.5 py-1 text-xs font-medium text-[var(--ui-accent)] transition hover:brightness-95"
          @click="emit('open-retention')"
        >
          <Settings2 :size="12" /> 调整
        </button>
      </div>
      <p class="mt-2 text-xs text-[var(--ui-text-muted)]">仅保存在本机数据目录</p>
    </GlassCard>
  </div>

  <details class="ui-disclosure">
    <summary>审计数据清理与保留策略 <span class="ui-muted">仅清理本地审计日志，不影响源码与任务</span></summary>
    <div class="ui-disclosure-body">
      <div class="ui-actions">
        <BaseButton variant="secondary" size="sm" @click="emit('open-retention')">
          <Clock3 :size="13" /> 修改保留天数（当前 {{ retentionDays }} 天）
        </BaseButton>
        <BaseButton v-if="workspaceId" variant="secondary" size="sm" @click="emit('clear-records', 'workspace')">
          <Trash2 :size="13" /> 清理当前筛选工作区日志
        </BaseButton>
        <BaseButton variant="danger" size="sm" @click="emit('clear-records', 'all')">
          <Trash2 :size="13" /> 清空全部审计记录
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
    <div class="space-y-4">
      <div>
        <label class="mb-2 block text-xs font-semibold text-[var(--ui-text)]">快速预设周期</label>
        <div class="grid grid-cols-3 gap-2.5">
          <button
            v-for="days in retentionPresets"
            :key="days"
            type="button"
            class="rounded-[var(--ui-radius-inset)] border px-3 py-2.5 text-center transition-colors"
            :class="
              retentionInput === days
                ? 'border-[var(--ui-accent)] bg-[var(--ui-accent-soft)] font-semibold text-[var(--ui-accent)]'
                : 'border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] text-xs text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)]'
            "
            @click="retentionInput = days"
          >
            <span class="block font-mono text-sm font-semibold">{{ days }} 天</span>
            <span class="mt-0.5 block text-[11px] text-[var(--ui-text-muted)]">
              {{ days === 30 ? "默认推荐" : days === 365 ? "长期归档" : `${Math.round(days / 7)} 周` }}
            </span>
          </button>
        </div>
      </div>

      <div class="ui-inset p-4">
        <label class="mb-1.5 block text-xs font-medium text-[var(--ui-text-secondary)]">自定义保留天数 (1 ~ 365 天)</label>
        <div class="flex items-center gap-2">
          <input
            v-model.number="retentionInput"
            type="number"
            min="1"
            max="365"
            class="h-9 w-28 rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] px-3 font-mono text-sm font-semibold text-[var(--ui-text)] outline-none focus:border-[var(--ui-accent)]"
          />
          <span class="text-xs text-[var(--ui-text-muted)]">天</span>
        </div>
      </div>

      <div class="flex items-center justify-end gap-2 pt-2">
        <BaseButton variant="ghost" size="sm" @click="retentionModalOpen = false">取消</BaseButton>
        <BaseButton variant="primary" size="sm" :busy="savingRetention" @click="emit('save-retention')">保存保留策略</BaseButton>
      </div>
    </div>
  </ModalDialog>
</template>
