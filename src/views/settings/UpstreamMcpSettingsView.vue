<script setup lang="ts">
import { AlertTriangle, Download, Plus, RefreshCw } from "@lucide/vue";
import UpstreamImportModal from "$src/components/settings/UpstreamImportModal.vue";
import UpstreamServerEditorModal from "$src/components/settings/UpstreamServerEditorModal.vue";
import UpstreamServerListCard from "$src/components/settings/UpstreamServerListCard.vue";
import UpstreamTakeoverBackupsCard from "$src/components/settings/UpstreamTakeoverBackupsCard.vue";
import UpstreamToolsModal from "$src/components/settings/UpstreamToolsModal.vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import { useUpstreamMcp } from "$lib/upstream-mcp/use-upstream-mcp";

const {
  loading,
  loadError,
  overview,
  globalBusy,
  syncingAll,
  busyServerId,
  testingServerId,
  directToolsOverLimit,
  editorOpen,
  editorSaving,
  editorError,
  editorDraft,
  toolsModalServerId,
  toolsModalServer,
  toolsSaving,
  importOpen,
  importTab,
  scanningClients,
  parsingJson,
  importingBusy,
  importError,
  importJsonText,
  activeImportCandidates,
  candidateSelections,
  replaceSourceConfigs,
  takeoverConnectMode,
  takeoverPreview,
  loadingTakeoverPreview,
  takeoverBackups,
  restoringBackupId,
  refresh,
  restoreBackup,
  updateGlobalSettings,
  openCreateModal,
  openEditModal,
  submitEditor,
  toggleServerEnabled,
  switchServerMode,
  testAndSyncServer,
  syncAllEnabled,
  removeServer,
  openToolsModal,
  saveDisabledTools,
  openImportModal,
  updateCandidateSelection,
  runLocalScan,
  runParseJson,
  submitImport,
} = useUpstreamMcp();
</script>

<template>
  <section id="settings-upstream-mcp" class="ui-stack" aria-label="MCP 聚合中心">
    <div v-if="loadError" class="ui-banner ui-banner--warning" role="alert">
      <span>加载 MCP 聚合状态失败：{{ loadError }}</span>
      <BaseButton variant="secondary" size="sm" @click="refresh">重试</BaseButton>
    </div>

    <p v-else-if="loading && !overview" role="status" class="ui-muted">正在加载聚合 MCP 配置…</p>

    <template v-if="overview">
      <!-- Top Summary & Global Controls -->
      <GlassCard variant="focal">
        <div class="flex flex-col gap-4">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="grid flex-1 grid-cols-2 gap-3 sm:grid-cols-4">
              <div class="rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-3 py-2.5">
                <div class="text-[11px] text-[var(--ui-text-muted)]">已挂载上游</div>
                <div class="mt-0.5 font-mono text-lg font-semibold text-[var(--ui-text)]">
                  {{ overview.servers.filter((s) => s.config.enabled).length }}
                  <span class="text-xs font-normal text-[var(--ui-text-muted)]">/ {{ overview.servers.length }}</span>
                </div>
              </div>
              <div class="rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-3 py-2.5">
                <div class="text-[11px] text-[var(--ui-text-muted)]">直出工具</div>
                <div class="mt-0.5 font-mono text-lg font-semibold text-[var(--ui-accent)]">
                  {{ overview.directExposedToolCount }}
                </div>
              </div>
              <div class="rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-3 py-2.5">
                <div class="text-[11px] text-[var(--ui-text-muted)]">代理元工具</div>
                <div class="mt-0.5 font-mono text-lg font-semibold text-[var(--ui-text)]">
                  {{ overview.proxyMetaToolCount }}
                </div>
              </div>
              <div class="rounded-[var(--ui-radius-control)] border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-3 py-2.5">
                <div class="text-[11px] text-[var(--ui-text-muted)]">总计对外工具</div>
                <div class="mt-0.5 font-mono text-lg font-semibold text-[var(--ui-text)]">
                  {{ overview.totalExposedToolCount }}
                  <span class="text-[11px] font-normal text-[var(--ui-text-muted)]">
                    (内置 {{ overview.builtinToolCount }})
                  </span>
                </div>
              </div>
            </div>
          </div>

          <div class="grid gap-3 border-t border-[var(--ui-line)] pt-3.5 sm:grid-cols-2">
            <ToggleSwitch
              :model-value="overview.enabled"
              :disabled="globalBusy"
              label="启用 MCP 聚合网关"
              description="开启后，所有连入 Global MCP 的客户端都可调用下方已启用的上游 MCP 工具"
              @update:model-value="updateGlobalSettings($event, overview.alwaysExposeProxyTools)"
            />
            <ToggleSwitch
              :model-value="overview.alwaysExposeProxyTools"
              :disabled="globalBusy || !overview.enabled"
              label="始终暴露代理元工具"
              description="即使全部服务为直出模式，也保留 mcp_upstream_list/tools/call 便于按关键词检索"
              @update:model-value="updateGlobalSettings(overview.enabled, $event)"
            />
          </div>

          <div
            v-if="directToolsOverLimit"
            class="flex items-start gap-2 rounded-[var(--ui-radius-control)] border border-[var(--ui-warning)]/30 bg-[var(--ui-warning-soft)] px-3 py-2 text-xs text-[var(--ui-warning)]"
          >
            <AlertTriangle :size="14" class="mt-0.5 shrink-0" />
            <span>
              当前直出模式累计暴露了 {{ overview.directExposedToolCount }} 个上游工具（总计 {{ overview.totalExposedToolCount }} 个），过多直出工具可能影响网页端 AI 客户端上下文或触达工具数上限。建议把工具较多的上游切换为「代理」模式。
            </span>
          </div>

          <div class="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--ui-line)] pt-3.5">
            <p class="text-xs text-[var(--ui-text-muted)]">
              提示：代理模式增删工具即时生效；直出模式增删工具后，部分外部客户端需刷新 MCP 连接。
            </p>
            <div class="flex flex-wrap items-center gap-2">
              <BaseButton variant="secondary" size="sm" @click="openImportModal('scan')">
                <Download :size="13" />
                扫描 / JSON 导入
              </BaseButton>
              <BaseButton
                variant="secondary"
                size="sm"
                :busy="syncingAll"
                :disabled="!overview.servers.some((s) => s.config.enabled)"
                @click="syncAllEnabled"
              >
                <RefreshCw v-if="!syncingAll" :size="13" />
                全部测试同步
              </BaseButton>
              <BaseButton variant="primary" size="sm" @click="openCreateModal">
                <Plus :size="13" />
                新增 MCP
              </BaseButton>
            </div>
          </div>
        </div>
      </GlassCard>

      <!-- Server List -->
      <UpstreamServerListCard
        :servers="overview.servers"
        :global-enabled="overview.enabled"
        :busy-server-id="busyServerId"
        :testing-server-id="testingServerId"
        :direct-warning-threshold="overview.directToolsWarningThreshold"
        @create="openCreateModal"
        @open-import="openImportModal"
        @edit="openEditModal"
        @delete="removeServer"
        @toggle-enabled="toggleServerEnabled"
        @switch-mode="switchServerMode"
        @test-sync="testAndSyncServer"
        @open-tools="openToolsModal"
      />

      <!-- Harness Takeover Backups & One-Click Restore -->
      <UpstreamTakeoverBackupsCard
        :backups="takeoverBackups"
        :restoring-backup-id="restoringBackupId"
        @restore="restoreBackup"
      />
    </template>

    <!-- Modals -->
    <UpstreamServerEditorModal
      :open="editorOpen"
      :initial-draft="editorDraft"
      :saving="editorSaving"
      :error="editorError"
      @close="editorOpen = false"
      @submit="submitEditor"
    />

    <UpstreamImportModal
      :open="importOpen"
      :tab="importTab"
      :scanning="scanningClients"
      :parsing="parsingJson"
      :importing="importingBusy"
      :error="importError"
      :json-text="importJsonText"
      :candidates="activeImportCandidates"
      :selections="candidateSelections"
      :replace-source-configs="replaceSourceConfigs"
      :connect-mode="takeoverConnectMode"
      :takeover-preview="takeoverPreview"
      :loading-takeover-preview="loadingTakeoverPreview"
      @close="importOpen = false"
      @update:tab="importTab = $event"
      @update:json-text="importJsonText = $event"
      @update:replace-source-configs="replaceSourceConfigs = $event"
      @update:connect-mode="takeoverConnectMode = $event"
      @update-selection="updateCandidateSelection($event.id, $event.field, $event.value)"
      @rescan="runLocalScan"
      @parse-json="runParseJson"
      @submit-import="submitImport"
    />

    <UpstreamToolsModal
      :server="toolsModalServer"
      :saving="toolsSaving"
      @close="toolsModalServerId = null"
      @save="saveDisabledTools"
    />
  </section>
</template>
