<script setup lang="ts">
import { Edit3, Link2, Monitor, Radar, RefreshCw, Search, Trash2 } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import ModalDialog from "$src/components/ui/ModalDialog.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import TextField from "$src/components/ui/TextField.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import type { DiscoveredPeerDto, PairedPeerStatusDto } from "$lib/api/lan-peers";
import { formatPlatformLabel } from "$lib/lan/use-lan-peers";

defineProps<{
  discoveredPeers: DiscoveredPeerDto[];
  pairedPeers: PairedPeerStatusDto[];
  scanning: boolean;
  probing: boolean;
  probeHost: string;
  probePort: string;
  probeError: string;
  peerActionId: string | null;
  pairingModalOpen: boolean;
  pairingTarget: {
    deviceName: string;
    host: string;
    port: number;
    os: string;
    fingerprint: string;
  } | null;
  pinInput: string;
  pairError: string;
  pairingBusy: boolean;
  renameModalOpen: boolean;
  renameInput: string;
}>();

const emit = defineEmits<{
  scan: [];
  "update:probeHost": [value: string];
  "update:probePort": [value: string];
  probe: [];
  "open-pair": [peer: DiscoveredPeerDto];
  "close-pair": [];
  "update:pinInput": [value: string];
  "submit-pair": [];
  "toggle-peer": [peer: PairedPeerStatusDto, enabled: boolean];
  "open-rename": [peer: PairedPeerStatusDto];
  "close-rename": [];
  "update:renameInput": [value: string];
  "submit-rename": [];
  unpair: [peer: PairedPeerStatusDto];
}>();
</script>

<template>
  <GlassCard>
    <div class="flex flex-wrap items-start justify-between gap-3">
      <div>
        <h2 class="text-base font-semibold">附近 / 组网可配对电脑与已互联设备</h2>
        <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
          自动通过广播与单播扫描局域网及 EasyTier / Tailscale 等虚拟组网内的 macOS / Windows 节点；也可直接输入组网 IP:端口 连接。
        </p>
      </div>
      <BaseButton variant="secondary" size="sm" :busy="scanning" @click="emit('scan')">
        <RefreshCw :size="13" />扫描并刷新状态
      </BaseButton>
    </div>

    <!-- Nearby Unpaired LAN Devices -->
    <div class="mt-4">
      <div class="flex items-center gap-1.5 text-xs font-semibold text-[var(--ui-text-secondary)]">
        <Radar :size="14" class="text-[var(--ui-accent)]" />
        <span>附近发现的未配对设备（{{ discoveredPeers.length }}）</span>
      </div>

      <div v-if="discoveredPeers.length" class="mt-2 grid gap-2">
        <div
          v-for="peer in discoveredPeers"
          :key="peer.deviceId"
          class="ui-inset flex flex-wrap items-center justify-between gap-3 p-3"
        >
          <div class="min-w-0">
            <div class="flex flex-wrap items-center gap-2">
              <strong class="text-sm">{{ peer.deviceName }}</strong>
              <span class="rounded-full border border-[var(--ui-line)] px-2 py-0.5 text-[11px] text-[var(--ui-text-secondary)]">
                {{ formatPlatformLabel(peer.os, peer.arch) }}
              </span>
            </div>
            <p class="mt-1 font-mono text-xs text-[var(--ui-text-muted)]">
              {{ peer.host }}:{{ peer.port }} · 指纹 {{ peer.fingerprint }}
            </p>
          </div>
          <BaseButton size="sm" @click="emit('open-pair', peer)">
            <Link2 :size="13" />输入配对码连接
          </BaseButton>
        </div>
      </div>
      <p v-else class="mt-2 rounded-[var(--ui-radius-row)] border border-dashed border-[var(--ui-line)] p-3 text-xs text-[var(--ui-text-muted)]">
        暂未发现未配对的局域网设备。请确认对方电脑已启动本地 MCP 服务并开启「局域网多设备对等配对」，或使用下方手动探测。
      </p>

      <!-- Manual IP:Port Fallback (Especially helpful when Windows Public Network Firewall blocks UDP) -->
      <details class="ui-disclosure mt-2.5">
        <summary>手动输入局域网 / EasyTier 组网 IP 探测配对 <span class="ui-muted">兼容跨子网组网 / Windows 防火墙隔离</span></summary>
        <div class="ui-disclosure-body">
          <div class="grid gap-2 sm:grid-cols-[1fr_130px_auto] sm:items-end">
            <TextField
              :model-value="probeHost"
              label="对方局域网或 EasyTier/Tailscale 组网 IP"
              placeholder="例如 10.144.144.2 或 192.168.1.36"
              :error="probeError"
              @update:model-value="emit('update:probeHost', $event)"
            />
            <TextField
              :model-value="probePort"
              label="MCP 端口"
              placeholder="28765"
              @update:model-value="emit('update:probePort', $event)"
            />
            <BaseButton variant="secondary" :busy="probing" @click="emit('probe')">
              <Search :size="13" />探测节点
            </BaseButton>
          </div>
        </div>
      </details>
    </div>

    <!-- Paired Devices List -->
    <div class="mt-5 border-t border-[var(--ui-line)] pt-4">
      <div class="flex items-center justify-between gap-2">
        <div class="flex items-center gap-1.5 text-xs font-semibold text-[var(--ui-text-secondary)]">
          <Monitor :size="14" class="text-[var(--ui-accent)]" />
          <span>已配对互信设备（{{ pairedPeers.length }}）</span>
        </div>
      </div>

      <div v-if="pairedPeers.length" class="mt-2.5 grid gap-2.5">
        <div
          v-for="peer in pairedPeers"
          :key="peer.deviceId"
          class="ui-inset p-3.5"
        >
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div class="min-w-0 flex-1">
              <div class="flex flex-wrap items-center gap-2">
                <strong class="text-sm">{{ peer.deviceName }}</strong>
                <StatusPill
                  :status="!peer.enabled ? 'paused' : peer.online ? 'healthy' : 'stopped'"
                  :label="!peer.enabled ? '已暂停' : peer.online ? '在线' : '离线'"
                />
                <span class="rounded-full border border-[var(--ui-line)] px-2 py-0.5 text-[11px] text-[var(--ui-text-secondary)]">
                  {{ formatPlatformLabel(peer.os, peer.arch) }}
                </span>
                <span
                  v-if="peer.online && peer.latencyMs !== null"
                  class="font-mono text-[11px] text-[var(--ui-success)]"
                >
                  {{ peer.latencyMs }} ms
                </span>
              </div>

              <p class="mt-1.5 font-mono text-xs text-[var(--ui-text-muted)] break-all">
                ID: {{ peer.deviceId }} · {{ peer.host }}:{{ peer.port }} · 指纹 {{ peer.fingerprint }}
              </p>

              <div class="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-[var(--ui-text-secondary)]">
                <span>共享工作区：<strong>{{ peer.workspaceCount }}</strong> 个</span>
                <span>•</span>
                <span>对方权限：{{ peer.trustLevel === "readonly" ? "仅只读" : "读写协作" }}</span>
                <template v-if="peer.computerUseSupported">
                  <span>•</span>
                  <span>
                    屏幕控制：{{ peer.computerUseEnabled ? "已就绪" : "本机未开启" }}
                  </span>
                </template>
              </div>

              <p v-if="!peer.online && peer.enabled && peer.lastError" class="mt-1 text-xs text-[var(--ui-text-muted)]">
                状态说明：{{ peer.lastError }}
              </p>
            </div>

            <div class="flex flex-wrap items-center gap-2 shrink-0">
              <ToggleSwitch
                :model-value="peer.enabled"
                :label="`启用 ${peer.deviceName}`"
                :disabled="peerActionId === peer.deviceId"
                @update:model-value="emit('toggle-peer', peer, $event)"
              />
              <BaseButton
                variant="ghost"
                size="sm"
                title="修改本地设备备注名称"
                :disabled="peerActionId === peer.deviceId"
                @click="emit('open-rename', peer)"
              >
                <Edit3 :size="13" />备注
              </BaseButton>
              <BaseButton
                variant="ghost"
                size="sm"
                title="解除双方配对"
                :disabled="peerActionId === peer.deviceId"
                @click="emit('unpair', peer)"
              >
                <Trash2 :size="13" class="text-[var(--ui-danger)]" />解除
              </BaseButton>
            </div>
          </div>
        </div>
      </div>

      <p v-else class="mt-2.5 text-xs text-[var(--ui-text-muted)]">
        尚未配对其他电脑。配对完成后，AI 客户端只需连接局域网内任意一台电脑的 MCP，即可自动发现并操作所有在线设备的工作区。
      </p>
    </div>

    <!-- Enter 6-Digit Pairing Code Modal -->
    <ModalDialog
      :open="pairingModalOpen"
      :title="pairingTarget ? `连接「${pairingTarget.deviceName}」` : '局域网设备配对'"
      :description="pairingTarget ? `请在目标电脑（${pairingTarget.host}:${pairingTarget.port} · 指纹 ${pairingTarget.fingerprint}）的设置页点击「生成 6 位配对码」，并在下方输入：` : ''"
      :dismissible="!pairingBusy"
      @close="emit('close-pair')"
    >
      <form class="ui-stack" @submit.prevent="emit('submit-pair')">
        <TextField
          :model-value="pinInput"
          label="6 位数字配对码"
          placeholder="例如 842916"
          :error="pairError"
          :disabled="pairingBusy"
          @update:model-value="emit('update:pinInput', $event)"
        />
        <div class="flex justify-end gap-2 mt-2">
          <BaseButton type="button" variant="ghost" :disabled="pairingBusy" @click="emit('close-pair')">
            取消
          </BaseButton>
          <BaseButton type="submit" :busy="pairingBusy">
            验证并双向配对
          </BaseButton>
        </div>
      </form>
    </ModalDialog>

    <!-- Rename Peer Alias Modal -->
    <ModalDialog
      :open="renameModalOpen"
      title="修改配对设备显示名称"
      description="自定义名称将显示在 AI 的 workspace_list 与 device_list 中，方便区分多台同名项目电脑。"
      @close="emit('close-rename')"
    >
      <form class="ui-stack" @submit.prevent="emit('submit-rename')">
        <TextField
          :model-value="renameInput"
          label="设备显示名称"
          placeholder="例如：书房 Mac mini / Windows 编译机"
          @update:model-value="emit('update:renameInput', $event)"
        />
        <div class="flex justify-end gap-2 mt-2">
          <BaseButton type="button" variant="ghost" @click="emit('close-rename')">取消</BaseButton>
          <BaseButton type="submit">保存名称</BaseButton>
        </div>
      </form>
    </ModalDialog>
  </GlassCard>
</template>
