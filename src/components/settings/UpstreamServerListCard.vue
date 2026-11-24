<script setup lang="ts">
import {
  AlertTriangle,
  Pencil,
  PlugZap,
  RefreshCw,
  SlidersHorizontal,
  Trash2,
} from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import EmptyState from "$src/components/ui/EmptyState.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import SegmentedControl from "$src/components/ui/SegmentedControl.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import type { UpstreamExposeMode, UpstreamServerOverviewDto } from "$lib/api/upstream-mcp";
import { transportLabel, upstreamStatusForPill } from "$lib/upstream-mcp/use-upstream-mcp";

defineProps<{
  servers: UpstreamServerOverviewDto[];
  globalEnabled: boolean;
  busyServerId: string | null;
  testingServerId: string | null;
  directWarningThreshold: number;
}>();

const emit = defineEmits<{
  create: [];
  "open-import": [tab: "scan" | "json"];
  edit: [item: UpstreamServerOverviewDto];
  delete: [item: UpstreamServerOverviewDto];
  "toggle-enabled": [item: UpstreamServerOverviewDto, enabled: boolean];
  "switch-mode": [item: UpstreamServerOverviewDto, mode: UpstreamExposeMode];
  "test-sync": [item: UpstreamServerOverviewDto];
  "open-tools": [item: UpstreamServerOverviewDto];
}>();

const MODE_OPTIONS = [
  { value: "direct", label: "直出" },
  { value: "proxy", label: "代理" },
];

function targetSummary(item: UpstreamServerOverviewDto): string {
  const t = item.config.transport;
  if (t.type === "stdio") {
    return t.args.length ? `${t.command} ${t.args.join(" ")}` : t.command;
  }
  return t.url;
}

function secretCount(item: UpstreamServerOverviewDto): number {
  const t = item.config.transport;
  if (t.type === "stdio") return t.secretEnvKeys.length;
  return t.secretHeaderKeys.length + (t.hasBearerToken ? 1 : 0);
}
</script>

<template>
  <GlassCard v-if="!servers.length">
    <EmptyState
      title="尚未挂载其他 MCP 服务"
      description="把本机 Stdio 或远程 HTTP / SSE MCP 聚合到 Coding Tools MCP 后，上层 AI 客户端只需连接这一个入口即可统一调用。"
    >
      <template #icon>
        <PlugZap :size="24" />
      </template>
      <div class="mt-4 flex flex-wrap items-center justify-center gap-2">
        <BaseButton variant="primary" size="sm" @click="emit('open-import', 'scan')">
          一键扫描本机客户端
        </BaseButton>
        <BaseButton variant="secondary" size="sm" @click="emit('open-import', 'json')">
          粘贴 mcpServers JSON
        </BaseButton>
        <BaseButton variant="ghost" size="sm" @click="emit('create')">
          手动添加
        </BaseButton>
      </div>
    </EmptyState>
  </GlassCard>

  <div v-else class="space-y-3">
    <GlassCard
      v-for="item in servers"
      :key="item.config.id"
      :padded="false"
      class="overflow-hidden transition-opacity"
      :class="{ 'opacity-65': !globalEnabled || !item.config.enabled }"
    >
      <div class="flex flex-col gap-3 p-4 sm:p-5">
        <div class="flex flex-wrap items-start justify-between gap-3">
          <div class="min-w-0 flex-1 space-y-1">
            <div class="flex flex-wrap items-center gap-2">
              <h3 class="text-[15px] font-semibold text-[var(--ui-text)]">
                {{ item.config.name }}
              </h3>
              <span
                class="rounded-md bg-[var(--ui-accent-soft)] px-1.5 py-0.5 font-mono text-[11px] font-semibold text-[var(--ui-accent)]"
                title="工具名前缀别名"
              >
                {{ item.config.alias }}
              </span>
              <span
                class="rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-2 py-0.5 text-[11px] text-[var(--ui-text-secondary)]"
              >
                {{ transportLabel(item.config.transport.type) }}
              </span>
              <StatusPill
                :status="upstreamStatusForPill(item.runtime.status).status"
                :label="upstreamStatusForPill(item.runtime.status).label"
              />
              <span
                v-if="item.runtime.lastLatencyMs !== null"
                class="font-mono text-[11px] text-[var(--ui-text-muted)]"
              >
                {{ item.runtime.lastLatencyMs }} ms
              </span>
              <span
                v-if="secretCount(item) > 0"
                class="rounded-full bg-[var(--ui-surface-subtle)] px-2 py-0.5 text-[11px] text-[var(--ui-text-muted)]"
              >
                {{ secretCount(item) }} 项加密凭据
              </span>
            </div>

            <p class="truncate font-mono text-xs text-[var(--ui-text-muted)]" :title="targetSummary(item)">
              {{ targetSummary(item) }}
            </p>
            <p v-if="item.config.description" class="text-xs text-[var(--ui-text-secondary)]">
              {{ item.config.description }}
            </p>
          </div>

          <div class="flex shrink-0 items-center gap-3">
            <SegmentedControl
              :items="MODE_OPTIONS"
              :model-value="item.config.exposeMode"
              :disabled="busyServerId === item.config.id"
              aria-label="切换暴露模式"
              @update:model-value="emit('switch-mode', item, $event as UpstreamExposeMode)"
            />
            <ToggleSwitch
              :model-value="item.config.enabled"
              :disabled="busyServerId === item.config.id"
              @update:model-value="emit('toggle-enabled', item, $event)"
            />
          </div>
        </div>

        <div
          v-if="item.runtime.lastError"
          class="flex items-start gap-2 rounded-[var(--ui-radius-control)] border border-[var(--ui-danger)]/25 bg-[var(--ui-danger-soft)] px-3 py-2 text-xs text-[var(--ui-danger)]"
          role="alert"
        >
          <AlertTriangle :size="14" class="mt-0.5 shrink-0" />
          <span class="min-w-0 break-all">{{ item.runtime.lastError }}</span>
        </div>

        <div
          v-else-if="
            item.config.enabled &&
            item.config.exposeMode === 'direct' &&
            item.enabledToolCount > directWarningThreshold
          "
          class="flex flex-wrap items-center justify-between gap-2 rounded-[var(--ui-radius-control)] border border-[var(--ui-warning)]/30 bg-[var(--ui-warning-soft)] px-3 py-2 text-xs text-[var(--ui-warning)]"
        >
          <span>
            该服务直出了 {{ item.enabledToolCount }} 个工具（建议单服务直出 ≤ {{ directWarningThreshold }} 个），建议切换为「代理」模式以节省上下文。
          </span>
          <BaseButton
            variant="secondary"
            size="sm"
            @click="emit('switch-mode', item, 'proxy')"
          >
            切为代理模式
          </BaseButton>
        </div>

        <div class="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--ui-line)] pt-3">
          <div class="flex flex-wrap items-center gap-2 text-xs text-[var(--ui-text-secondary)]">
            <span v-if="item.runtime.syncedAtMs">
              工具：
              <strong class="font-mono text-[var(--ui-text)]">
                {{ item.enabledToolCount }} / {{ item.totalToolCount }}
              </strong>
              已启用
              <span v-if="item.config.exposeMode === 'direct'" class="text-[var(--ui-text-muted)]">
                （前缀 <code class="font-mono">{{ item.config.alias }}__*</code>）
              </span>
              <span v-else class="text-[var(--ui-text-muted)]">
                （经 <code class="font-mono">mcp_upstream_*</code> 按需调用）
              </span>
            </span>
            <span v-else class="text-[var(--ui-text-muted)]">
              尚未同步工具清单，点击右侧“测试同步”拉取
            </span>
          </div>

          <div class="flex flex-wrap items-center gap-1.5">
            <BaseButton
              variant="secondary"
              size="sm"
              :busy="testingServerId === item.config.id"
              :disabled="busyServerId === item.config.id"
              @click="emit('test-sync', item)"
            >
              <RefreshCw v-if="testingServerId !== item.config.id" :size="13" />
              测试同步
            </BaseButton>
            <BaseButton
              variant="secondary"
              size="sm"
              :disabled="item.totalToolCount === 0"
              @click="emit('open-tools', item)"
            >
              <SlidersHorizontal :size="13" />
              工具过滤
            </BaseButton>
            <BaseButton
              variant="ghost"
              size="sm"
              @click="emit('edit', item)"
            >
              <Pencil :size="13" />
              编辑
            </BaseButton>
            <BaseButton
              variant="ghost"
              size="sm"
              :disabled="busyServerId === item.config.id"
              title="移除上游 MCP"
              @click="emit('delete', item)"
            >
              <Trash2 :size="13" class="text-[var(--ui-danger)]" />
            </BaseButton>
          </div>
        </div>
      </div>
    </GlassCard>
  </div>
</template>
