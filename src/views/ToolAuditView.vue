<script setup lang="ts">
import { AlertTriangle, ChevronLeft, ChevronRight, FilterX, RefreshCw, RotateCcw, ShieldCheck } from "@lucide/vue";
import AuditFilterBar from "$src/components/audit/AuditFilterBar.vue";
import AuditOverviewAndRetention from "$src/components/audit/AuditOverviewAndRetention.vue";
import AuditRecordRow from "$src/components/audit/AuditRecordRow.vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import PageHeader from "$src/components/ui/PageHeader.vue";
import { useToolAudit } from "$lib/audit/use-tool-audit";

const {
  workspaces,
  PAGE_SIZE,
  loading,
  savingRetention,
  records,
  total,
  errorCount,
  workspaceCount,
  offset,
  retentionDays,
  retentionInput,
  healthMessage,
  query,
  workspaceId,
  sessionId,
  toolName,
  outcome,
  timePreset,
  fromDate,
  toDate,
  showAdvancedFilters,
  retentionModalOpen,
  expandedKeys,
  copiedKey,
  workspaceNames,
  selectedWorkspaceName,
  pageNumber,
  pageCount,
  rangeStart,
  rangeEnd,
  successRate,
  outcomeSegments,
  timePresets,
  retentionPresets,
  activeFilters,
  loadAudit,
  applyFilters,
  handleTimePresetChange,
  resetFilters,
  getRecordKey,
  toggleExpand,
  copyText,
  quickFilterBySession,
  quickFilterByTool,
  quickFilterByWorkspace,
  openRetentionModal,
  saveRetention,
  clearRecords,
  formatTime,
  formatRelativeTime,
  formatDuration,
  formatBytes,
  authLabel,
  errorLabel,
} = useToolAudit();

const auditFormatters = {
  errorLabel,
  formatBytes,
  formatTime,
  formatRelativeTime,
  formatDuration,
  authLabel,
};
</script>

<template>
  <section class="page-scroll min-h-0 flex-1">
    <div class="tool-audit-page ui-page">
      <PageHeader
        kicker="全局可观测与安全审计"
        title="工具调用审计"
        description="追溯经过网关的每一次工具调用、跨机转发状态、耗时与变更影响；日志仅在本机持久化。"
      >
        <template #actions>
          <BaseButton variant="secondary" size="sm" :busy="loading" @click="loadAudit(0)">
            <RefreshCw v-if="!loading" :size="14" />
            <span>刷新数据</span>
          </BaseButton>
        </template>
      </PageHeader>

      <div v-if="healthMessage" class="ui-banner ui-banner--warning text-sm" role="status">
        <AlertTriangle :size="18" class="mt-0.5 shrink-0 text-[var(--ui-warning)]" />
        <div class="min-w-0 flex-1">
          <p class="text-xs font-semibold text-[var(--ui-warning)]">审计日志写入异常通知</p>
          <p class="mt-0.5 text-xs leading-5 text-[var(--ui-text-secondary)]">
            {{ healthMessage }} 提示：工具调用仍在继续正常执行，仅部分日志写盘被推迟。
          </p>
        </div>
      </div>

      <AuditOverviewAndRetention
        v-model:retention-modal-open="retentionModalOpen"
        v-model:retention-input="retentionInput"
        :total="total"
        :error-count="errorCount"
        :workspace-count="workspaceCount"
        :page-number="pageNumber"
        :page-count="pageCount"
        :success-rate="successRate"
        :selected-workspace-name="selectedWorkspaceName"
        :workspace-id="workspaceId"
        :retention-days="retentionDays"
        :retention-presets="retentionPresets"
        :saving-retention="savingRetention"
        @open-retention="openRetentionModal"
        @save-retention="saveRetention"
        @clear-records="clearRecords"
      />

      <GlassCard :padded="false" class="overflow-hidden">
        <AuditFilterBar
          v-model:query="query"
          v-model:outcome="outcome"
          v-model:workspace-id="workspaceId"
          v-model:tool-name="toolName"
          v-model:session-id="sessionId"
          v-model:from-date="fromDate"
          v-model:to-date="toDate"
          v-model:show-advanced-filters="showAdvancedFilters"
          :workspaces="workspaces"
          :outcome-segments="outcomeSegments"
          :time-presets="timePresets"
          :active-filters="activeFilters"
          :time-preset="timePreset"
          @apply="applyFilters"
          @reset="resetFilters"
          @select-time-preset="handleTimePresetChange"
        />

        <div
          v-if="loading && records.length === 0"
          class="flex min-h-[300px] flex-col items-center justify-center gap-3 text-sm text-[var(--ui-text-muted)]"
        >
          <div class="grid h-12 w-12 place-items-center rounded-2xl bg-[var(--ui-surface-subtle)]">
            <RefreshCw :size="20" class="animate-spin text-[var(--ui-accent)]" />
          </div>
          <p class="text-xs font-medium">正在读取审计日志记录…</p>
        </div>

        <div
          v-else-if="records.length === 0"
          class="flex min-h-[260px] flex-col items-center justify-center px-6 text-center"
        >
          <div class="mb-3 grid h-14 w-14 place-items-center rounded-full bg-[var(--ui-surface-subtle)] text-[var(--ui-text-muted)]">
            <FilterX v-if="activeFilters.length > 0" :size="24" />
            <ShieldCheck v-else :size="24" />
          </div>
          <p class="text-sm font-semibold text-[var(--ui-text)]">
            {{ activeFilters.length > 0 ? "未匹配到符合条件的审计记录" : "暂无工具调用审计记录" }}
          </p>
          <p class="mt-1.5 max-w-md text-xs leading-relaxed text-[var(--ui-text-muted)]">
            {{
              activeFilters.length > 0
                ? "建议调整关键词、时间范围或放宽过滤条件后重试。"
                : "当 MCP 客户端发起工具调用时，调用耗时、认证方式与变更摘要将自动汇总在此。"
            }}
          </p>
          <BaseButton v-if="activeFilters.length > 0" variant="secondary" size="sm" class="mt-4" @click="resetFilters">
            <RotateCcw :size="13" /> 重置筛选条件
          </BaseButton>
        </div>

        <div v-else class="divide-y divide-[var(--ui-line)]">
          <AuditRecordRow
            v-for="record in records"
            :key="getRecordKey(record)"
            :record="record"
            :expanded="expandedKeys.has(getRecordKey(record))"
            :copied-key="expandedKeys.has(getRecordKey(record)) ? (copiedKey || null) : null"
            :workspace-name="record.workspaceId ? workspaceNames.get(record.workspaceId) || record.workspaceId : '全局路由'"
            :formatters="auditFormatters"
            @toggle="toggleExpand(getRecordKey(record))"
            @filter-session="quickFilterBySession"
            @filter-tool="quickFilterByTool"
            @filter-workspace="quickFilterByWorkspace"
            @copy-text="copyText"
          />
        </div>

        <footer class="flex flex-wrap items-center justify-between gap-3 border-t border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-5 py-3.5">
          <div class="flex items-center gap-2 text-xs tabular-nums text-[var(--ui-text-muted)]">
            <span>
              显示第 <strong class="font-medium text-[var(--ui-text)]">{{ rangeStart }}–{{ rangeEnd }}</strong> 条
            </span>
            <span>·</span>
            <span>
              共 <strong class="font-medium text-[var(--ui-text)]">{{ total.toLocaleString() }}</strong> 条记录
            </span>
          </div>

          <div class="flex items-center gap-2">
            <BaseButton variant="secondary" size="sm" :disabled="offset === 0 || loading" @click="loadAudit(Math.max(0, offset - PAGE_SIZE))">
              <ChevronLeft :size="14" /> 上一页
            </BaseButton>

            <span class="min-w-16 text-center font-mono text-xs font-medium text-[var(--ui-text-secondary)]">
              {{ pageNumber }} / {{ pageCount }}
            </span>

            <BaseButton variant="secondary" size="sm" :disabled="offset + PAGE_SIZE >= total || loading" @click="loadAudit(offset + PAGE_SIZE)">
              下一页 <ChevronRight :size="14" />
            </BaseButton>
          </div>
        </footer>
      </GlassCard>

      <footer class="flex flex-wrap items-center justify-between gap-2 px-1 text-xs text-[var(--ui-text-muted)]">
        <p>隐私保护说明：仅在本机记录工具名称、耗时与文件变更计数，不保存提示词原文与完整返回内容。</p>
        <span class="shrink-0 font-mono text-[11px]">MCP Audit v2.5</span>
      </footer>
    </div>
  </section>
</template>
