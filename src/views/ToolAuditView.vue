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
  <section class="page-scroll flex-1 min-h-0">
    <div class="tool-audit-page ui-page space-y-6 pb-12">
      <PageHeader
        kicker="全局可观测与审计"
        title="工具调用审计"
        description="追溯经过已认证 MCP 网关的工具调用、执行状态、响应耗时与数据变更。日志仅在本机持久化。"
      >
        <template #actions>
          <BaseButton variant="secondary" size="sm" :busy="loading" @click="loadAudit(0)">
            <RefreshCw :size="13" :class="{ 'animate-spin': loading }" /> 刷新数据
          </BaseButton>
        </template>
      </PageHeader>

      <transition
        enter-active-class="transition duration-200 ease-out"
        enter-from-class="opacity-0 -translate-y-2"
        enter-to-class="opacity-100 translate-y-0"
      >
        <div v-if="healthMessage" class="ui-banner ui-banner--warning text-sm" role="status">
          <AlertTriangle :size="18" class="mt-0.5 shrink-0 text-[var(--ui-warning)]" />
          <div class="min-w-0 flex-1">
            <p class="font-semibold text-xs text-[var(--ui-warning)]">审计日志写入异常通知</p>
            <p class="mt-0.5 text-xs leading-5 text-[var(--ui-text-secondary)]">
              {{ healthMessage }} 提示：工具调用仍在继续正常执行，仅部分日志写盘被推迟。
            </p>
          </div>
        </div>
      </transition>

      <GlassCard :padded="false" class="overflow-hidden border border-[var(--border)] shadow-sm animate-fade-in-up delay-200">
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
          class="flex min-h-[320px] flex-col items-center justify-center gap-3 text-sm text-[var(--text-muted)]"
        >
          <div class="flex h-12 w-12 items-center justify-center rounded-2xl bg-[var(--surface-hover)]">
            <RefreshCw :size="22" class="animate-spin text-[var(--primary)]" />
          </div>
          <p class="font-medium">正在读取审计日志记录…</p>
        </div>

        <div
          v-else-if="records.length === 0"
          class="flex min-h-[220px] flex-col items-center justify-center px-6 text-center sm:min-h-[320px]"
        >
          <div class="flex h-14 w-14 items-center justify-center rounded-2xl bg-[var(--surface-hover)] text-[var(--text-muted)] mb-3">
            <FilterX v-if="activeFilters.length > 0" :size="26" />
            <ShieldCheck v-else :size="26" />
          </div>
          <p class="text-sm font-semibold text-[var(--text-main)]">
            {{ activeFilters.length > 0 ? '未匹配到符合条件的审计记录' : '暂无工具调用审计记录' }}
          </p>
          <p class="mt-1.5 max-w-md text-xs leading-relaxed text-[var(--text-muted)]">
            {{ activeFilters.length > 0
              ? '建议调整关键词、时间范围或放宽过滤条件重试。'
              : '当已授权的 MCP 客户端调用工具时，调用记录将自动捕获并列于此处。' }}
          </p>
          <BaseButton
            v-if="activeFilters.length > 0"
            variant="secondary"
            size="sm"
            class="mt-4"
            @click="resetFilters"
          >
            <RotateCcw :size="13" /> 重置所有筛选条件
          </BaseButton>
        </div>

        <div v-else class="divide-y divide-[var(--border-light)]">
          <AuditRecordRow
            v-for="record in records"
            :key="getRecordKey(record)"
            :record="record"
            :expanded="expandedKeys.has(getRecordKey(record))"
            :copied-key="copiedKey"
            :workspace-name="record.workspaceId ? workspaceNames.get(record.workspaceId) || record.workspaceId : '全局路由'"
            :formatters="auditFormatters"
            @toggle="toggleExpand(getRecordKey(record))"
            @filter-session="quickFilterBySession"
            @filter-tool="quickFilterByTool"
            @filter-workspace="quickFilterByWorkspace"
            @copy-text="copyText"
          />
        </div>

        <footer class="flex flex-wrap items-center justify-between gap-3 border-t border-[var(--border-light)] bg-[var(--ui-surface-subtle)] px-4 py-3 lg:px-5">
          <div class="flex items-center gap-2 text-xs text-[var(--text-muted)]">
            <span>显示第 <strong class="font-medium text-[var(--text-main)]">{{ rangeStart }}–{{ rangeEnd }}</strong> 条</span>
            <span>/</span>
            <span>共 <strong class="font-medium text-[var(--text-main)]">{{ total.toLocaleString() }}</strong> 条记录</span>
          </div>

          <div class="flex items-center gap-2">
            <BaseButton
              variant="secondary"
              size="sm"
              :disabled="offset === 0 || loading"
              @click="loadAudit(Math.max(0, offset - PAGE_SIZE))"
            >
              <ChevronLeft :size="14" /> 上一页
            </BaseButton>

            <span class="min-w-16 text-center text-xs font-mono font-medium text-[var(--text-secondary)]">
              {{ pageNumber }} / {{ pageCount }}
            </span>

            <BaseButton
              variant="secondary"
              size="sm"
              :disabled="offset + PAGE_SIZE >= total || loading"
              @click="loadAudit(offset + PAGE_SIZE)"
            >
              下一页 <ChevronRight :size="14" />
            </BaseButton>
          </div>
        </footer>
      </GlassCard>

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

      <footer class="flex items-center justify-between px-1 text-xs leading-relaxed text-[var(--text-muted)]">
        <p>
          💡 审计日志安全说明：记录保存在本机应用数据目录。为保护用户隐私，仅持久化工具名称、耗时与变更摘要，不保存请求提示词参数与完整模型输出内容。
        </p>
        <span class="shrink-0 font-mono text-xs">MCP Audit v2.4</span>
      </footer>
    </div>
  </section>
</template>
