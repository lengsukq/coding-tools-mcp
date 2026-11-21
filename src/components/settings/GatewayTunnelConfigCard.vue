<script setup lang="ts">
import { RotateCw } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import SaveBar from "$src/components/ui/SaveBar.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import TextField from "$src/components/ui/TextField.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import type { GlobalGatewayConfigDto } from "$lib/api/global-gateway";
import { isValidPort } from "$lib/validators";

defineProps<{
  config: GlobalGatewayConfigDto;
  tunnelState: string;
  tunnelLabel?: string;
  tunnelOptions: Array<{ value: string; label: string }>;
  frpOptions: Array<{ value: string; label: string }>;
  canForceRestartCloudflare: boolean;
  tunnelActionError: string;
  dirty: boolean;
  saving: boolean;
  busy: boolean;
  loading: boolean;
  checking: boolean;
  loadError: string;
  error: string;
}>();

const emit = defineEmits<{
  "update-config": [patch: Partial<GlobalGatewayConfigDto>];
  save: [];
  "restart-cloudflare": [];
}>();
</script>

<template>
  <GlassCard>
    <div class="section-heading mb-4 flex flex-wrap items-start justify-between gap-3">
      <div>
        <h2 class="text-base font-semibold">公网隧道配置</h2>
        <p class="mt-1 text-xs text-[var(--ui-text-muted)]">所有工作区共享本地端点；FRP 和 Cloudflare 用于提供公网访问。</p>
      </div>
      <StatusPill :status="tunnelState" :label="tunnelLabel" />
    </div>

    <fieldset :disabled="saving || busy || loading || !!loadError" class="grid gap-3">
      <div class="ui-inset p-3">
        <ToggleSwitch
          :model-value="config.enabled"
          label="启用公网连接"
          description="关闭时仍可正常使用本地 MCP 服务，只是不自动暴露公网入口。"
          @update:model-value="emit('update-config', { enabled: $event })"
        />
      </div>
      <div class="settings-two-col">
        <TextField
          :model-value="String(config.localPort)"
          label="MCP 本地端口"
          :error="error && !isValidPort(config.localPort) ? error : undefined"
          type="number"
          @update:model-value="emit('update-config', { localPort: Number($event) })"
        />
        <SelectField
          :model-value="config.tunnelType"
          label="公网方式"
          :options="tunnelOptions"
          @update:model-value="emit('update-config', { tunnelType: $event })"
        />
      </div>

      <template v-if="config.tunnelType === 'frp'">
        <SelectField
          :model-value="config.frpProfileId"
          label="FRP 配置"
          :options="frpOptions"
          @update:model-value="emit('update-config', { frpProfileId: $event })"
        />
        <TextField
          :model-value="config.frpSubdomain"
          label="子域名"
          placeholder="coding-tools"
          @update:model-value="emit('update-config', { frpSubdomain: $event })"
        />
        <div v-if="!config.frpProfileId" class="settings-two-col">
          <TextField
            :model-value="config.frpServer"
            label="FRP 服务器"
            placeholder="frp.example.com"
            @update:model-value="emit('update-config', { frpServer: $event })"
          />
          <TextField
            :model-value="String(config.frpServerPort)"
            label="端口"
            :error="error && !isValidPort(config.frpServerPort) ? error : undefined"
            type="number"
            @update:model-value="emit('update-config', { frpServerPort: Number($event) })"
          />
        </div>
      </template>

      <div v-if="config.tunnelType === 'cloudflare'" class="ui-banner text-xs leading-5">
        Cloudflare Quick Tunnel 会分配临时公网地址；地址变化后，需要把新的 `/mcp` 地址重新填入外部 Connector。
      </div>

      <div v-if="canForceRestartCloudflare" class="tunnel-action-row ui-inset p-3">
        <div class="min-w-0 flex-1">
          <strong class="text-xs">Cloudflare Tunnel 运维</strong>
          <p class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]">
            强制终止当前受管理的隧道进程并获取新的临时公网地址，只重启隧道，不会重启本地 MCP 服务。
          </p>
          <p v-if="tunnelActionError" class="ui-danger mt-2 break-words text-xs" role="alert" aria-live="assertive">
            重启失败：{{ tunnelActionError }}
          </p>
        </div>
        <BaseButton
          variant="secondary"
          :busy="busy"
          :disabled="loading || !!loadError || checking"
          aria-label="强制重启 Cloudflare Tunnel"
          @click="emit('restart-cloudflare')"
        >
          <RotateCw v-if="!busy" :size="13" />强制重启隧道
        </BaseButton>
      </div>

      <TextField
        v-if="config.tunnelType === 'none'"
        :model-value="config.publicUrl"
        label="外部公网 URL（可选）"
        placeholder="https://gateway.example.com"
        @update:model-value="emit('update-config', { publicUrl: $event })"
      />

      <div v-if="config.tunnelType !== 'none'" class="ui-inset p-3">
        <ToggleSwitch
          :model-value="config.useProxy"
          label="使用网络代理"
          description="使用网络与工具页的网络代理连接公网隧道。"
          @update:model-value="emit('update-config', { useProxy: $event })"
        />
      </div>

      <SaveBar :dirty="dirty" :busy="saving" :error="error" label="保存连接配置" @save="emit('save')" />
    </fieldset>
  </GlassCard>
</template>

<style scoped>
.section-heading { min-width: 0; }
.tunnel-action-row {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}
.settings-two-col {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: .75rem;
  min-width: 0;
}
@container app-main (max-width: 700px) {
  .settings-two-col { grid-template-columns: 1fr; }
}
@container app-main (max-width: 480px) {
  .tunnel-action-row { align-items: flex-start; flex-wrap: wrap; }
  .tunnel-action-row > button { margin-left: auto; }
}
</style>
