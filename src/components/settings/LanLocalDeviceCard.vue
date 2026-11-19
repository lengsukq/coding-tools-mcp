<script setup lang="ts">
import { Check, Copy, KeyRound, Laptop, ShieldCheck, X } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import SaveBar from "$src/components/ui/SaveBar.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import TextAreaField from "$src/components/ui/TextAreaField.vue";
import TextField from "$src/components/ui/TextField.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import type { LocalDeviceDto, PairingPinDto } from "$lib/api/lan-peers";
import type { WorkspaceProfile } from "$lib/types";
import { formatPlatformLabel, type LanDraftForm } from "$lib/lan/use-lan-peers";

defineProps<{
  localDevice: LocalDeviceDto | null;
  mcpRunning: boolean;
  onlinePeerCount: number;
  pairedTotalCount: number;
  draft: LanDraftForm;
  workspaces: WorkspaceProfile[];
  dirty: boolean;
  saving: boolean;
  saveError: string;
  activePin: PairingPinDto | null;
  pinCountdownSec: number;
  pinBusy: boolean;
  copiedKey: string | null;
}>();

const emit = defineEmits<{
  save: [];
  "generate-pin": [];
  "revoke-pin": [];
  "toggle-workspace": [workspaceId: string, checked: boolean];
  "copy-text": [text: string, key: string, label: string];
}>();

const shareModeOptions = [
  { value: "all", label: "共享本机全部工作区（推荐）" },
  { value: "selected", label: "仅共享勾选的指定工作区" },
];

const trustLevelOptions = [
  { value: "full", label: "完全协作（读写与命令均遵循本机工作区权限）" },
  { value: "readonly", label: "仅只读共享（禁止远端配对电脑写入或执行命令）" },
];

function spacedPin(pin: string): string {
  return pin.split("").join(" ");
}

function formatAddressList(device: LocalDeviceDto): string {
  return device.addresses.map((ip) => `${ip}:${device.mcpPort}`).join(" · ");
}
</script>

<template>
  <GlassCard>
    <div class="flex flex-wrap items-start justify-between gap-4">
      <div class="flex min-w-0 items-center gap-3">
        <div class="grid h-11 w-11 shrink-0 place-items-center rounded-[var(--ui-radius-row)] bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]">
          <Laptop :size="20" />
        </div>
        <div class="min-w-0">
          <div class="flex flex-wrap items-center gap-2">
            <h2 class="text-base font-semibold">局域网与异地组网多设备配对</h2>
            <StatusPill
              :status="draft.enabled ? (mcpRunning ? 'running' : 'warning') : 'stopped'"
              :label="draft.enabled ? (mcpRunning ? `${onlinePeerCount}/${pairedTotalCount} 台在线` : '待启动 MCP') : '未启用'"
            />
          </div>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            支持同一局域网及 EasyTier / Tailscale / ZeroTier 跨子网虚拟组网；配对后本机 Agent 免 OAuth 即可安全访问全网 macOS / Windows 工作区。
          </p>
        </div>
      </div>

      <div class="flex flex-wrap items-center gap-2">
        <BaseButton
          v-if="!activePin"
          variant="secondary"
          size="sm"
          :busy="pinBusy"
          @click="emit('generate-pin')"
        >
          <KeyRound :size="14" />生成 6 位配对码
        </BaseButton>
        <BaseButton
          v-else
          variant="ghost"
          size="sm"
          :busy="pinBusy"
          @click="emit('revoke-pin')"
        >
          <X :size="14" />关闭配对码
        </BaseButton>
      </div>
    </div>

    <!-- Active 6-digit Pairing Code Banner -->
    <div
      v-if="activePin && pinCountdownSec > 0"
      class="mt-4 rounded-[var(--ui-radius-row)] border border-[var(--ui-accent)] bg-[var(--ui-accent-soft)]/55 p-4"
      role="region"
      aria-label="当前配对码"
    >
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <span class="text-xs font-medium text-[var(--ui-accent)]">
            本机一次性配对码（剩余 {{ pinCountdownSec }} 秒 · 可尝试 {{ activePin.remainingAttempts }} 次）
          </span>
          <div class="mt-1.5 font-mono text-2xl font-bold tracking-[0.22em] text-[var(--ui-text)]">
            {{ spacedPin(activePin.pin) }}
          </div>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            请在局域网或 EasyTier 组网内另一台电脑点击连接本机并输入此配对码，验证成功后双方自动完成双向互信。
          </p>
        </div>
        <BaseButton
          variant="secondary"
          size="sm"
          @click="emit('copy-text', activePin.pin, 'active-pin', '6 位配对码')"
        >
          <Check v-if="copiedKey === 'active-pin'" :size="13" class="text-[var(--ui-success)]" />
          <Copy v-else :size="13" />
          {{ copiedKey === "active-pin" ? "已复制" : "复制配对码" }}
        </BaseButton>
      </div>
    </div>

    <!-- Local Device Identity & Local Agent Direct Endpoint Summary -->
    <div v-if="localDevice" class="mt-4 grid gap-2 sm:grid-cols-2">
      <div class="ui-inset p-3">
        <div class="flex items-center justify-between gap-2">
          <span class="text-xs text-[var(--ui-text-muted)]">本机设备标识 · {{ formatPlatformLabel(localDevice.os, localDevice.arch) }}</span>
          <button
            type="button"
            class="inline-flex items-center gap-1 text-xs text-[var(--ui-text-muted)] hover:text-[var(--ui-text)]"
            @click="emit('copy-text', localDevice.deviceId, 'device-id', '设备 ID')"
          >
            <Check v-if="copiedKey === 'device-id'" :size="12" class="text-[var(--ui-success)]" />
            <Copy v-else :size="12" />
            复制 ID
          </button>
        </div>
        <code class="mt-1 block break-all text-xs font-semibold">{{ localDevice.deviceId }}</code>
        <span class="mt-1 flex items-center gap-1 text-[11px] text-[var(--ui-text-muted)]">
          <ShieldCheck :size="12" class="text-[var(--ui-success)]" />
          安全指纹：{{ localDevice.fingerprint }}
        </span>
      </div>

      <div class="ui-inset p-3">
        <div class="flex items-center justify-between gap-2">
          <span class="text-xs text-[var(--ui-text-muted)]">本机局域网 / 组网可达地址（端口 {{ localDevice.mcpPort }}）</span>
          <button
            v-if="localDevice.addresses.length"
            type="button"
            class="inline-flex items-center gap-1 text-xs text-[var(--ui-text-muted)] hover:text-[var(--ui-text)]"
            @click="emit('copy-text', `${localDevice.addresses[0]}:${localDevice.mcpPort}`, 'lan-addr', '节点地址')"
          >
            <Check v-if="copiedKey === 'lan-addr'" :size="12" class="text-[var(--ui-success)]" />
            <Copy v-else :size="12" />
            复制地址
          </button>
        </div>
        <code v-if="localDevice.addresses.length" class="mt-1 block break-all text-xs">
          {{ formatAddressList(localDevice) }}
        </code>
        <span v-else class="mt-1 block text-xs text-[var(--ui-text-muted)]">未检测到活动局域网或虚拟组网 IPv4</span>
        <span v-if="draft.enabled && !mcpRunning" class="mt-1 block text-[11px] text-[var(--ui-warning)]">
          提示：请启动上方本地 MCP 服务，以便其他电脑连接本机。
        </span>
      </div>

      <div class="ui-inset p-3 sm:col-span-2">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div class="min-w-0">
            <span class="text-xs text-[var(--ui-text-muted)]">
              本机 Agent 直连地址（{{ draft.allowLocalNoauth || localDevice.authType === "noauth" ? "已启用本机免 OAuth 直连 · 自动连通全部配对工作区" : "当前需按全局认证模式提供凭据" }}）
            </span>
            <code class="mt-1 block break-all text-xs">{{ localDevice.localAgentEndpoint }}</code>
          </div>
          <BaseButton
            variant="ghost"
            size="sm"
            @click="emit('copy-text', localDevice.localAgentEndpoint, 'local-agent-url', '本机 Agent 地址')"
          >
            <Check v-if="copiedKey === 'local-agent-url'" :size="13" class="text-[var(--ui-success)]" />
            <Copy v-else :size="13" />
            {{ copiedKey === "local-agent-url" ? "已复制" : "复制本机 Agent 地址" }}
          </BaseButton>
        </div>
      </div>
    </div>

    <!-- Editable Federation Policy -->
    <div class="mt-4 ui-stack">
      <ToggleSwitch
        v-model="draft.enabled"
        label="开启局域网 / 组网对等发现与工作区联邦"
        description="开启后在物理网卡及虚拟组网网卡上监听对等配对请求（配对前不暴露任何工作区信息，公网隧道请求一律拦截）。"
      />

      <ToggleSwitch
        v-model="draft.allowLocalNoauth"
        label="允许本机 Agent 免 OAuth 直连并调用局域网/组网工作区"
        description="开启后，本机 Claude Code、Cursor、Codex、Windsurf 等客户端直连 127.0.0.1 无需 OAuth 授权，跨机请求由网关自动以配对密钥签名转发。"
      />

      <div class="ui-two-col">
        <TextField
          v-model="draft.deviceName"
          label="本机显示名称（用于区分多台电脑）"
          placeholder="例如：Leo 的 MacBook Pro / 客厅 Mac mini / Windows 主力机"
        />
        <SelectField
          v-model="draft.trustLevel"
          label="向已配对设备授予的访问权限"
          :options="trustLevelOptions"
        />
      </div>

      <SelectField
        v-model="draft.shareMode"
        label="向已配对设备暴露的工作区范围"
        :options="shareModeOptions"
      />

      <div v-if="draft.shareMode === 'selected'" class="ui-inset p-3 space-y-2">
        <span class="block text-xs font-medium text-[var(--ui-text-secondary)]">勾选允许配对电脑访问的本机工作区：</span>
        <p v-if="!workspaces.length" class="text-xs text-[var(--ui-text-muted)]">当前尚未创建本机工作区。</p>
        <label
          v-for="ws in workspaces"
          :key="ws.id"
          class="ui-checkbox-row text-xs"
        >
          <input
            type="checkbox"
            :checked="draft.sharedWorkspaceIds.includes(ws.id)"
            @change="emit('toggle-workspace', ws.id, ($event.target as HTMLInputElement).checked)"
          />
          <span class="font-medium">{{ ws.name }}</span>
          <code class="text-[11px] text-[var(--ui-text-muted)] truncate">{{ ws.path }}</code>
        </label>
      </div>

      <details class="ui-disclosure">
        <summary>
          跨子网与组网工具配置 <span class="ui-muted">EasyTier / Tailscale / ZeroTier / WireGuard / 多子网 VLAN</span>
        </summary>
        <div class="ui-disclosure-body">
          <TextAreaField
            v-model="draft.extraSubnetsText"
            label="自定义组网子网段（CIDR）或固定节点地址（每行或逗号分隔，可选）"
            placeholder="10.144.144.0/24&#10;192.168.2.0/24&#10;10.144.144.2:28765"
            :rows="3"
            mono
          />
          <p class="text-xs text-[var(--ui-text-muted)]">
            默认已自动识别常见虚拟网卡网段（EasyTier 默认 <code>10.144.144.0/24</code>、<code>10.126.126.0/24</code>、<code>198.18.0.0/15</code> 与 Tailscale <code>100.64.0.0/10</code>）。若使用了 EasyTier 子网代理（无 TUN 跨网段转发）或自定义网段，在此填入目标 CIDR（如 <code>192.168.2.0/24</code>）或节点 IP 即可自动扫描发现。
          </p>
        </div>
      </details>

      <ToggleSwitch
        v-if="localDevice?.computerUseSupported"
        v-model="draft.allowComputerUse"
        label="允许已配对设备转发 Computer Use 请求至本机"
        description="仍严格受本机「Computer Use」页面的手动开关、macOS 录屏/辅助功能权限及敏感动作二次确认保护。"
      />

      <SaveBar :dirty="dirty" :busy="saving" :error="saveError" @save="emit('save')" />
    </div>
  </GlassCard>
</template>
