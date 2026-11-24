<script setup lang="ts">
import { Activity, Check, Copy, Globe2, Play, RefreshCw, Square } from "@lucide/vue";
import GatewayHealthCard from "$src/components/settings/GatewayHealthCard.vue";
import GatewayTunnelConfigCard from "$src/components/settings/GatewayTunnelConfigCard.vue";
import GeminiAppConnector from "$src/components/settings/GeminiAppConnector.vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import { getGlobalRuntimeSettings } from "$lib/api/settings";
import { useGatewaySettings } from "$lib/settings/use-gateway-settings";
import { recordConnectionHealth } from "$lib/stores/health";

const props = withDefaults(
  defineProps<{
    panel?: "gateway" | "gemini";
  }>(),
  { panel: "gateway" },
);

const {
  config,
  error,
  loadError,
  healthError,
  tunnelActionError,
  health,
  authType,
  mcpServerName,
  loading,
  saving,
  busy,
  checking,
  healthCheckFailed,
  copiedEndpoint,
  running,
  canForceRestartCloudflare,
  dirty,
  mcpState,
  mcpDetail,
  tunnelState,
  tunnelLabel,
  tunnelDetail,
  localEndpoint,
  publicEndpoint,
  publicEndpointAvailable,
  publicEndpointStale,
  geminiPublicHealth,
  healthSummary,
  healthSummaryStatus,
  lastCheckedLabel,
  tunnelOptions,
  frpOptions,
  patchConfig,
  refresh,
  saveConfig,
  start,
  stop,
  restartCloudflareTunnel,
  runHealth,
  copyEndpoint,
} = useGatewaySettings({
  panel: () => props.panel,
  loadRuntimeSettings: getGlobalRuntimeSettings,
  onRecordHealth: recordConnectionHealth,
});
</script>

<template>
  <div id="settings-gateway" class="ui-stack">
    <div v-if="loadError" class="ui-banner ui-banner--warning" role="alert" aria-live="assertive">
      <span>{{ loadError }}</span>
      <BaseButton variant="secondary" @click="refresh()">重试</BaseButton>
    </div>

    <template v-if="panel === 'gemini'">
      <GeminiAppConnector
        :endpoint="publicEndpoint"
        :local-endpoint="localEndpoint"
        :server-name="mcpServerName"
        :public-enabled="config.enabled"
        :mcp-running="running"
        :auth-type="authType"
        :public-health="geminiPublicHealth"
        :config-dirty="dirty"
      />
    </template>

    <template v-else>
      <GlassCard>
        <div class="section-heading flex flex-wrap items-start justify-between gap-4">
          <div class="flex min-w-0 items-center gap-3">
            <div class="grid h-11 w-11 shrink-0 place-items-center rounded-[var(--ui-radius-row)] bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]">
              <Globe2 :size="20" />
            </div>
            <div class="min-w-0">
              <h2 class="text-base font-semibold">MCP 服务与端点</h2>
              <p class="mt-1 text-xs text-[var(--ui-text-muted)]">本地服务与公网隧道独立管理，修改配置后支持运行中平滑重载。</p>
            </div>
          </div>
          <div class="status-summary-grid" aria-label="连接状态">
            <div class="status-summary">
              <span class="text-xs text-[var(--ui-text-muted)]">本地 MCP</span>
              <StatusPill :status="mcpState" />
              <span class="text-xs text-[var(--ui-text-muted)]">{{ mcpDetail }}</span>
            </div>
            <div class="status-summary">
              <span class="text-xs text-[var(--ui-text-muted)]">公网隧道</span>
              <StatusPill :status="tunnelState" :label="tunnelLabel" />
              <span class="text-xs text-[var(--ui-text-muted)]">{{ tunnelDetail }}</span>
            </div>
          </div>
        </div>

        <div class="mt-4 grid gap-2 sm:grid-cols-2">
          <div class="endpoint-row ui-inset p-3">
            <div class="min-w-0 flex-1">
              <span class="text-xs text-[var(--ui-text-muted)]">本地 MCP 端点</span>
              <code class="mt-1 block break-all text-xs">{{ localEndpoint }}</code>
            </div>
            <BaseButton
              variant="ghost"
              size="sm"
              :aria-label="copiedEndpoint === 'local' ? '本地端点已复制' : '复制本地端点'"
              @click="copyEndpoint('local')"
            >
              <Check v-if="copiedEndpoint === 'local'" :size="13" class="text-[var(--ui-success)]" />
              <Copy v-else :size="13" />
              {{ copiedEndpoint === "local" ? "已复制" : "复制" }}
            </BaseButton>
          </div>
          <div class="endpoint-row ui-inset p-3" :class="publicEndpointStale ? 'endpoint-row--stale' : undefined">
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-2">
                <span class="text-xs text-[var(--ui-text-muted)]">公网 MCP 端点</span>
                <span v-if="publicEndpointStale" class="text-[11px] text-[var(--ui-warning)]">最近地址</span>
              </div>
              <code class="mt-1 block break-all text-xs">{{ publicEndpoint }}</code>
            </div>
            <BaseButton
              v-if="publicEndpointAvailable"
              variant="ghost"
              size="sm"
              :aria-label="copiedEndpoint === 'public' ? '公网端点已复制' : '复制公网端点'"
              @click="copyEndpoint('public')"
            >
              <Check v-if="copiedEndpoint === 'public'" :size="13" class="text-[var(--ui-success)]" />
              <Copy v-else :size="13" />
              {{ copiedEndpoint === "public" ? "已复制" : "复制" }}
            </BaseButton>
          </div>
        </div>

        <div class="mt-4 flex flex-wrap gap-2">
          <BaseButton :busy="busy && !running" :disabled="running || loading" @click="start">
            <Play v-if="!(busy && !running)" :size="13" />启动 MCP 服务
          </BaseButton>
          <BaseButton variant="secondary" :busy="busy && running" :disabled="!running || busy" @click="stop">
            <Square v-if="!(busy && running)" :size="12" />停止 MCP 服务
          </BaseButton>
          <BaseButton variant="ghost" :busy="checking" :disabled="busy" @click="runHealth">
            <Activity v-if="!checking" :size="13" />健康检查
          </BaseButton>
          <BaseButton variant="ghost" :busy="loading" :disabled="busy" @click="refresh()">
            <RefreshCw v-if="!loading" :size="13" />刷新状态
          </BaseButton>
        </div>
      </GlassCard>

      <div class="ui-bento-grid items-start">
        <GatewayTunnelConfigCard
          :config="config"
          :tunnel-state="tunnelState"
          :tunnel-label="tunnelLabel"
          :tunnel-options="tunnelOptions"
          :frp-options="frpOptions"
          :can-force-restart-cloudflare="canForceRestartCloudflare"
          :tunnel-action-error="tunnelActionError"
          :dirty="dirty"
          :saving="saving"
          :busy="busy"
          :loading="loading"
          :checking="checking"
          :load-error="loadError"
          :error="error"
          @update-config="patchConfig"
          @save="saveConfig"
          @restart-cloudflare="restartCloudflareTunnel"
        />

        <GatewayHealthCard
          :health="health"
          :health-summary-status="healthSummaryStatus"
          :health-summary="healthSummary"
          :last-checked-label="lastCheckedLabel"
          :health-error="healthError"
          :checking="checking"
          :health-check-failed="healthCheckFailed"
          :busy="busy"
          :can-force-restart-cloudflare="canForceRestartCloudflare"
          @restart-cloudflare="restartCloudflareTunnel"
        />
      </div>
    </template>
  </div>
</template>

<style scoped>
.section-heading { min-width: 0; }
.status-summary-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(150px, 1fr));
  gap: 8px;
  min-width: min(100%, 340px);
}
.status-summary {
  display: grid;
  grid-template-columns: auto auto;
  align-items: center;
  gap: 4px 8px;
  min-width: 0;
  padding: 10px 12px;
  border: 1px solid var(--ui-line);
  border-radius: var(--ui-radius-row);
  background: var(--ui-surface-subtle);
}
.status-summary > span:last-child { grid-column: 1 / -1; overflow-wrap: anywhere; }
.endpoint-row {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}
.endpoint-row--stale { border-color: color-mix(in srgb, var(--ui-warning) 35%, var(--ui-line)); }
@container app-main (max-width: 700px) {
  .status-summary-grid { grid-template-columns: 1fr; min-width: 0; width: 100%; }
}
@container app-main (max-width: 480px) {
  .endpoint-row { align-items: flex-start; flex-wrap: wrap; }
  .endpoint-row > button { margin-left: auto; }
}
</style>
