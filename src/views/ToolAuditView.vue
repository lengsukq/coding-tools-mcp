<script setup lang="ts">
import { Activity, AlertCircle, AlertTriangle, CheckCircle2, ChevronDown, ChevronLeft, ChevronRight, Clock3, Filter, FilterX, HardDrive, Layers, RefreshCw, RotateCcw, Search, Settings2, ShieldCheck, Trash2 } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import ModalDialog from "$src/components/ui/ModalDialog.vue";
import PageHeader from "$src/components/ui/PageHeader.vue";
import SegmentedControl from "$src/components/ui/SegmentedControl.vue";
import { useToolAudit } from "$lib/audit/use-tool-audit";
import AuditRecordRow from "$src/components/audit/AuditRecordRow.vue";

const { workspaces, PAGE_SIZE, loading, savingRetention, clearingRecords, records, total, errorCount, workspaceCount, offset, retentionDays, retentionInput, healthMessage, query, workspaceId, sessionId, toolName, outcome, timePreset, fromDate, toDate, showAdvancedFilters, retentionModalOpen, clearConfirmOpen, expandedKeys, copiedKey, workspaceNames, selectedWorkspaceName, pageNumber, pageCount, rangeStart, rangeEnd, successCount, successRate, outcomeSegments, timePresets, retentionPresets, activeFilters, loadAudit, applyFilters, handleTimePresetChange, resetFilters, getRecordKey, toggleExpand, copyText, quickFilterBySession, quickFilterByTool, quickFilterByWorkspace, openRetentionModal, saveRetention, clearRecords, formatTime, formatRelativeTime, formatDuration, formatBytes, authLabel, errorLabel } = useToolAudit();
const auditFormatters = { errorLabel, formatBytes, formatTime, formatRelativeTime, formatDuration, authLabel };
</script>

<template>
  <section class="page-scroll flex-1 min-h-0">
    <div class="tool-audit-page ui-page space-y-6 pb-12">

      <!-- Header -->
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

      <!-- Health Warning Banner -->
      <transition enter-active-class="transition duration-200 ease-out" enter-from-class="opacity-0 -translate-y-2" enter-to-class="opacity-100 translate-y-0">
        <div
          v-if="healthMessage"
          class="ui-banner ui-banner--warning text-sm"
          role="status"
        >
          <AlertTriangle :size="18" class="mt-0.5 shrink-0 text-[var(--ui-warning)]" />
          <div class="min-w-0 flex-1">
            <p class="font-semibold text-xs text-[var(--ui-warning)]">审计日志写入异常通知</p>
            <p class="mt-0.5 text-xs leading-5 text-[var(--ui-text-secondary)]">{{ healthMessage }} 提示：工具调用仍在继续正常执行，仅部分日志写盘被推迟。</p>
          </div>
        </div>
      </transition>

      <!-- Secondary summary stays collapsed so filters and records remain in view. -->
      <!-- Main Audit Explorer Console -->
      <GlassCard :padded="false" class="overflow-hidden border border-[var(--border)] shadow-sm animate-fade-in-up delay-200">
        <!-- Filter Toolbar -->
        <div class="border-b border-[var(--border-light)] bg-[var(--ui-surface-subtle)] p-4 lg:p-5 space-y-3.5">
          <!-- Top Row: Search + Outcome Segments + Time Range + Actions -->
          <div class="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
            <!-- Search bar -->
            <div class="relative flex-1 min-w-[240px] max-w-xl">
              <Search :size="14" class="pointer-events-none absolute left-3 top-1/2 -translate-y-1/2 text-[var(--text-muted)]" />
              <input
                v-model="query"
                type="text"
                aria-label="搜索审计记录" placeholder="搜索工具名称、Request ID、Session ID、客户端…"
                class="w-full h-9 rounded-xl border border-[var(--border)] bg-[var(--card-bg)] pl-9 pr-8 text-xs text-[var(--text-main)] placeholder-[var(--text-muted)] outline-none transition-all focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--primary)]/15"
                @keydown.enter="applyFilters"
              />
              <button
                v-if="query"
                type="button"
                class="absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)] hover:text-[var(--text-main)]"
                @click="query = ''; applyFilters()"
              >
                <X :size="13" />
              </button>
            </div>

            <!-- Segments & Quick Presets -->
            <div class="flex flex-wrap items-center gap-2">
              <!-- Outcome Segmented Control -->
              <SegmentedControl
                v-model="outcome"
                :items="outcomeSegments"
                aria-label="执行结果状态切换"
                @update:model-value="applyFilters"
              />

              <!-- Advanced Filter Toggle -->
              <button
                type="button"
                class="inline-flex h-8 items-center gap-1.5 rounded-full border border-[var(--border)] px-3 text-xs font-medium text-[var(--text-secondary)] transition hover:bg-[var(--surface-hover)] hover:text-[var(--text-main)]"
                :class="{ 'border-[var(--primary)] text-[var(--primary)] bg-[var(--primary-soft)]': showAdvancedFilters }"
                @click="showAdvancedFilters = !showAdvancedFilters"
              >
                <Filter :size="13" />
                <span>高级过滤</span>
                <ChevronDown :size="12" class="transition-transform duration-200" :class="{ 'rotate-180': showAdvancedFilters }" />
              </button>

              <BaseButton variant="primary" size="sm" @click="applyFilters">
                <Search :size="13" /> 检索
              </BaseButton>
            </div>
          </div>

          <!-- Time Presets Pills -->
          <div class="flex flex-wrap items-center gap-1.5 pt-0.5">
            <span class="text-xs font-medium text-[var(--text-muted)] mr-1">时间范围:</span>
            <button
              v-for="preset in timePresets"
              :key="preset.value"
              type="button"
              class="rounded-full px-3 py-1 text-xs font-medium transition-all"
              :class="timePreset === preset.value
                ? 'bg-[var(--ui-solid-accent)] text-[var(--ui-on-accent)] shadow-sm'
                : 'text-[var(--text-secondary)] border border-[var(--card-border)] hover:bg-[var(--primary-soft)] hover:text-[var(--text-main)]'"
              @click="handleTimePresetChange(preset.value)"
            >
              {{ preset.label }}
            </button>
          </div>

          <!-- Advanced Filters Drawer -->
          <transition
            enter-active-class="transition duration-200 ease-out"
            enter-from-class="opacity-0 -translate-y-2 max-h-0"
            enter-to-class="opacity-100 translate-y-0 max-h-96"
            leave-active-class="transition duration-150 ease-in"
            leave-from-class="opacity-100 translate-y-0 max-h-96"
            leave-to-class="opacity-0 -translate-y-2 max-h-0"
          >
            <div v-if="showAdvancedFilters" class="grid gap-3 pt-2 border-t border-[var(--border-light)] sm:grid-cols-2 lg:grid-cols-4">
              <!-- Workspace Select -->
              <div>
                <label for="audit-workspaceId" class="block text-xs font-medium text-[var(--text-muted)] mb-1">所属工作区</label>
                <div class="relative">
                  <select
                    id="audit-workspaceId" v-model="workspaceId"
                    class="w-full h-8 appearance-none rounded-lg border border-[var(--border)] bg-[var(--card-bg)] pl-3 pr-8 text-xs text-[var(--text-main)] outline-none transition-all focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--primary)]/15 cursor-pointer"
                    @change="applyFilters"
                  >
                    <option value="">全部工作区与全局调用</option>
                    <option v-for="ws in workspaces" :key="ws.id" :value="ws.id">
                      {{ ws.name }}
                    </option>
                  </select>
                  <ChevronDown
                    :size="13"
                    class="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-[var(--text-muted)]"
                  />
                </div>
              </div>

              <!-- Tool Name Input -->
              <div>
                <label for="audit-toolName" class="block text-xs font-medium text-[var(--text-muted)] mb-1">工具名称精确匹配</label>
                <input
                  id="audit-toolName" v-model="toolName"
                  type="text"
                  placeholder="如: edit_file, execute_command"
                  class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-3 text-xs text-[var(--text-main)] placeholder-[var(--text-muted)] outline-none transition-all focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--primary)]/15"
                  @keydown.enter="applyFilters"
                />
              </div>

              <!-- Session ID Input -->
              <div>
                <label for="audit-sessionId" class="block text-xs font-medium text-[var(--text-muted)] mb-1">Session ID 关联</label>
                <input
                  id="audit-sessionId" v-model="sessionId"
                  type="text"
                  placeholder="匹配会话会签标识"
                  class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-3 text-xs text-[var(--text-main)] placeholder-[var(--text-muted)] outline-none transition-all focus:border-[var(--primary)] focus:ring-2 focus:ring-[var(--primary)]/15"
                  @keydown.enter="applyFilters"
                />
              </div>

              <!-- Custom Date Range -->
              <div v-if="timePreset === 'custom'" class="sm:col-span-2 lg:col-span-1">
                <label class="block text-xs font-medium text-[var(--text-muted)] mb-1">自定义起止时间</label>
                <div class="flex items-center gap-1.5">
                  <input
                    aria-label="起始时间" v-model="fromDate"
                    type="datetime-local"
                    class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-2 text-xs text-[var(--text-main)] outline-none focus:border-[var(--primary)]"
                    @change="applyFilters"
                  />
                  <span class="text-[var(--text-muted)] text-xs">至</span>
                  <input
                    aria-label="结束时间" v-model="toDate"
                    type="datetime-local"
                    class="w-full h-8 rounded-lg border border-[var(--border)] bg-[var(--card-bg)] px-2 text-xs text-[var(--text-main)] outline-none focus:border-[var(--primary)]"
                    @change="applyFilters"
                  />
                </div>
              </div>
            </div>
          </transition>

          <!-- Active Filter Pills Bar -->
          <div v-if="activeFilters.length > 0" class="flex flex-wrap items-center gap-1.5 pt-1">
            <span class="text-xs font-medium text-[var(--text-muted)]">生效筛选:</span>
            <span
              v-for="filter in activeFilters"
              :key="filter.id"
              class="inline-flex items-center gap-1 rounded-md bg-[var(--surface-hover)] border border-[var(--border)] px-2 py-0.5 text-xs text-[var(--text-main)]"
            >
              {{ filter.label }}
              <button
                type="button"
                class="text-[var(--text-muted)] hover:text-[var(--ui-danger)] transition-colors"
                @click="filter.clear"
              >
                <X :size="11" />
              </button>
            </span>

            <button
              type="button"
              class="inline-flex items-center gap-1 text-xs text-[var(--primary)] hover:underline ml-1"
              @click="resetFilters"
            >
              <RotateCcw :size="11" /> 清空所有筛选
            </button>
          </div>
        </div>

        <!-- Records Feed Area -->
        <!-- Loading State -->
        <div v-if="loading && records.length === 0" class="flex min-h-[320px] flex-col items-center justify-center gap-3 text-sm text-[var(--text-muted)]">
          <div class="flex h-12 w-12 items-center justify-center rounded-2xl bg-[var(--surface-hover)]">
            <RefreshCw :size="22" class="animate-spin text-[var(--primary)]" />
          </div>
          <p class="font-medium">正在读取审计日志记录…</p>
        </div>

        <!-- Empty State -->
        <div v-else-if="records.length === 0" class="flex min-h-[220px] flex-col items-center justify-center px-6 text-center sm:min-h-[320px]">
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

        <!-- Records List -->
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

        <!-- Pagination Footer -->
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

      <details class="audit-kpis">
        <summary>
          <span class="audit-kpis__title">审计概览</span>
          <span class="audit-kpis__hint">{{ total.toLocaleString() }} 条匹配记录 · 成功率 {{ successRate === null ? '暂无' : `${successRate}%` }} · 保留 {{ retentionDays }} 天</span>
        </summary>
        <div class="grid grid-cols-1 gap-3 pt-3 sm:grid-cols-2 lg:grid-cols-4 animate-fade-in-up delay-100">
        <!-- 1. Total Calls -->
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

        <!-- 2. Success Rate -->
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
          <!-- Progress bar -->
          <div v-if="successRate !== null" class="mt-2.5 h-1.5 w-full overflow-hidden rounded-full bg-[var(--ui-surface-subtle)]">
            <div
              class="h-full rounded-full transition-all duration-500 ease-out"
              :class="successRate !== null && successRate >= 95 ? 'bg-[var(--ui-success)]' : successRate !== null && successRate >= 80 ? 'bg-[var(--ui-warning)]' : 'bg-[var(--ui-danger)]'"
              :style="{ width: `${successRate ?? 0}%` }"
            />
          </div>
        </GlassCard>

        <!-- 3. Workspaces -->
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

        <!-- 4. Retention Policy -->
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
              @click="openRetentionModal"
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

<details class="ui-disclosure"><summary>审计管理</summary><div class="ui-disclosure-body"><p class="ui-muted">清理仅删除本地审计记录，不修改源码与计划；删除后无法恢复。</p><div class="ui-actions">          <BaseButton variant="ghost" size="sm" @click="openRetentionModal">
            <Clock3 :size="13" /> 保留 {{ retentionDays }} 天
          </BaseButton>

          <BaseButton
            v-if="workspaceId"
            variant="ghost"
            size="sm"
            @click="clearRecords('workspace')"
          >
            <Trash2 :size="13" /> 清理当前工作区
          </BaseButton>

          <BaseButton variant="danger" size="sm" @click="clearRecords('all')">
            <Trash2 :size="13" /> 清空记录
          </BaseButton>
</div></div></details>
      <!-- Bottom System Notes -->
      <footer class="flex items-center justify-between px-1 text-xs leading-relaxed text-[var(--text-muted)]">
        <p>
          💡 审计日志安全说明：记录保存在本机应用数据目录。为保护用户隐私，仅持久化工具名称、耗时与变更摘要，不保存请求提示词参数与完整模型输出内容。
        </p>
        <span class="shrink-0 font-mono text-xs">MCP Audit v2.4</span>
      </footer>

    </div>

    <!-- Retention Configuration Modal -->
    <ModalDialog
      :open="retentionModalOpen"
      title="配置审计日志保留期限"
      description="超过保留天数的历史工具调用记录将在应用启动或定时轮转中自动安全清理。"
      @close="retentionModalOpen = false"
    >
      <div class="space-y-4 pt-1">
        <!-- Preset Days Selection -->
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

        <!-- Custom Days Input -->
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

        <!-- Actions -->
        <div class="flex items-center justify-end gap-2 pt-2 border-t border-[var(--border-light)]">
          <BaseButton variant="ghost" size="sm" @click="retentionModalOpen = false">
            取消
          </BaseButton>
          <BaseButton variant="primary" size="sm" :busy="savingRetention" @click="saveRetention">
            保存保留策略
          </BaseButton>
        </div>
      </div>
    </ModalDialog>
  </section>
</template>
