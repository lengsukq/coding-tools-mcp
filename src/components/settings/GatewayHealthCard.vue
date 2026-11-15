<script setup lang="ts">
import { RotateCw } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import type { GatewayHealthItemDto } from "$lib/api/global-gateway";
import { connectionHealthPresentation } from "$lib/ui-model";

const props = defineProps<{
  health: GatewayHealthItemDto[];
  healthSummaryStatus: string;
  healthSummary: string;
  lastCheckedLabel: string;
  healthError: string;
  checking: boolean;
  healthCheckFailed: boolean;
  busy: boolean;
  canForceRestartCloudflare: boolean;
}>();

const emit = defineEmits<{
  "restart-cloudflare": [];
}>();

function shouldOfferTunnelRecovery(item: GatewayHealthItemDto) {
  return props.canForceRestartCloudflare && item.label.includes("公网") && !item.ok;
}
</script>

<template>
  <GlassCard id="settings-health">
    <div class="section-heading flex flex-wrap items-start justify-between gap-3">
      <div>
        <h2 class="text-base font-semibold">连接健康</h2>
        <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
          检查本地与公网 MCP 端点；进程运行中不等于公网请求一定可达。
        </p>
      </div>
      <div class="health-summary" aria-live="polite">
        <StatusPill :status="healthSummaryStatus" :label="healthSummary" />
        <span class="text-xs text-[var(--ui-text-muted)]">{{ lastCheckedLabel }}</span>
      </div>
    </div>

    <p v-if="healthError" class="ui-danger mt-3 break-words text-xs" role="alert" aria-live="assertive">
      检查失败：{{ healthError }}。已有结果会保留，点击健康检查重试。
    </p>

    <div class="mt-3 space-y-2">
      <div v-for="item in health" :key="item.label" class="health-row ui-inset p-3">
        <div class="min-w-0 flex-1">
          <div class="flex flex-wrap items-center gap-2">
            <strong class="text-xs">{{ item.label }}</strong>
            <StatusPill
              :status="connectionHealthPresentation(item).status"
              :label="connectionHealthPresentation(item).label"
            />
          </div>
          <p class="mt-1 break-all text-xs leading-5 text-[var(--ui-text-muted)]">{{ item.detail }}</p>
        </div>
        <BaseButton
          v-if="shouldOfferTunnelRecovery(item)"
          variant="ghost"
          size="sm"
          :busy="busy"
          :disabled="checking"
          @click="emit('restart-cloudflare')"
        >
          <RotateCw :size="12" />重启隧道
        </BaseButton>
      </div>

      <p v-if="health.length === 0" class="py-4 text-center text-xs text-[var(--ui-text-muted)]" aria-live="polite">
        {{ checking ? "正在检查连接…" : healthCheckFailed ? "连接健康未知" : "暂无连接检查结果" }}
      </p>
    </div>
  </GlassCard>
</template>

<style scoped>
.section-heading { min-width: 0; }
.health-row {
  display: flex;
  align-items: flex-start;
  gap: 12px;
  min-width: 0;
}
.health-summary {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
}
@container app-main (max-width: 700px) {
  .health-summary { justify-content: flex-start; }
}
@container app-main (max-width: 480px) {
  .health-row { flex-wrap: wrap; }
  .health-row > button { margin-left: auto; }
}
</style>
