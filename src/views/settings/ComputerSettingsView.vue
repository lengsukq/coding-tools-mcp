<script setup lang="ts">
import ComputerAuditPanel from "$src/components/settings/ComputerAuditPanel.vue";
import ComputerDiagnosticsBanner from "$src/components/settings/ComputerDiagnosticsBanner.vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import {
  MANUAL_DURATION_OPTIONS,
  MANUAL_SCOPE_OPTIONS,
  useComputerSettings,
} from "$lib/computer/use-computer-settings";

const {
  computer,
  computerBusy,
  computerAppOptions,
  selectedComputerApp,
  manualScope,
  manualDuration,
  manualTargetApp,
  confirmPermanent,
  computerAuditRecords,
  computerAuditHealth,
  auditBusy,
  manualExpiryLabel,
  screenPermissionTroubleshooting,
  refreshComputerAudit,
  refreshComputerApps,
  enableManualComputer,
  decideSensitiveAction,
  toggleComputerPause,
  requestAccessibilityPermission,
  requestScreenPermission,
  refreshComputerScreen,
  approveComputer,
  revokeComputer,
} = useComputerSettings();
</script>

<template>
  <div id="settings-computer" class="ui-stack">
    <GlassCard>
      <div class="section-heading flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 class="text-base font-semibold">Computer Use · 屏幕访问</h2>
          <p class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]">
            默认关闭。本机手动开启后，已通过 MCP OAuth / Bearer 验证的客户端可直接在授权范围内查看或操作，无需逐次申请；所有调用记录客户端身份和结果。
            查看与控制权限分离，控制只作用于本机指定应用，macOS 屏幕录制与辅助功能授权仍然有效。
          </p>
        </div>
        <StatusPill
          :status="computer?.active && !computer?.paused ? 'running' : 'stopped'"
          :label="computer?.paused ? '已暂停' : computer?.manualEnabled ? '本机主动授权' : computer?.active ? '旧会话授权' : '未授权'"
        />
      </div>

      <div class="ui-inset mt-4 grid gap-3 p-3">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div>
            <strong class="text-sm">本机主动授权</strong>
            <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
              {{ manualExpiryLabel }} · {{
                computer?.manualEnabled
                  ? computer.manualScope === "control"
                    ? "控制指定应用"
                    : "仅查看屏幕"
                  : "尚未开启"
              }}
            </p>
          </div>
          <div v-if="computer?.manualEnabled" class="flex flex-wrap gap-2">
            <BaseButton variant="secondary" :disabled="computerBusy" @click="toggleComputerPause">
              {{ computer.manualPaused ? "恢复" : "紧急暂停" }}
            </BaseButton>
            <BaseButton variant="secondary" :disabled="computerBusy" @click="revokeComputer">
              立即关闭
            </BaseButton>
          </div>
        </div>

        <div class="settings-two-col">
          <SelectField v-model="manualScope" label="权限范围" :options="MANUAL_SCOPE_OPTIONS" />
          <SelectField v-model="manualDuration" label="授权时间" :options="MANUAL_DURATION_OPTIONS" />
        </div>

        <div v-if="manualScope === 'control'" class="settings-two-col">
          <SelectField v-model="manualTargetApp" label="只允许操作此应用" :options="computerAppOptions" />
          <div class="flex items-end">
            <BaseButton variant="ghost" :disabled="computerBusy" @click="refreshComputerApps">
              刷新应用列表
            </BaseButton>
          </div>
        </div>

        <p v-if="manualScope === 'control'" class="text-xs leading-5 text-[var(--ui-text-muted)]">
          控制模式仅针对所选应用；鼠标坐标仍需近期截图，删除、支付、发送等敏感操作仍需你单独确认。切换客户端会使旧截图失效。
        </p>

        <div v-if="manualDuration === '0'" class="ui-inset p-3">
          <ToggleSwitch
            v-model="confirmPermanent"
            label="我确认永久授权"
            description="永久授权在应用重启后继续生效，直到在这里手动关闭。只建议用于你信任的 MCP 客户端与网络环境。"
          />
        </div>

        <div class="flex flex-wrap gap-2">
          <BaseButton
            :busy="computerBusy"
            :disabled="!computer?.supported || computerBusy || (manualScope === 'control' && (!manualTargetApp || !computer?.accessibilityGranted)) || (manualDuration === '0' && !confirmPermanent)"
            @click="enableManualComputer"
          >
            {{ computer?.manualEnabled ? "更新本机授权" : "开启 Computer Use" }}
          </BaseButton>
          <BaseButton v-if="computer?.manualEnabled" variant="ghost" :disabled="computerBusy" @click="revokeComputer">
            撤销全部访问
          </BaseButton>
        </div>
      </div>

      <div
        v-if="computer?.active && computer.pendingSensitiveActionId"
        class="ui-inset mt-3 flex flex-wrap items-center gap-3 p-3"
      >
        <div class="min-w-0 flex-1">
          <strong class="text-xs">高风险操作需单独确认</strong>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            目标应用：{{ computer.pendingSensitiveActionApp }} · {{ computer.pendingSensitiveActionDescription }}
          </p>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            {{
              computer.pendingSensitiveActionApproved
                ? "已允许一次，等待原客户端重试执行；不可重复使用"
                : "审批本身不会执行操作"
            }} · 剩余 {{ computer.pendingSensitiveActionExpiresInSeconds ?? 0 }} 秒
          </p>
        </div>
        <BaseButton
          v-if="!computer.pendingSensitiveActionApproved"
          :disabled="computerBusy || computer.paused"
          @click="decideSensitiveAction(true)"
        >
          允许本次
        </BaseButton>
        <BaseButton variant="secondary" :disabled="computerBusy" @click="decideSensitiveAction(false)">
          拒绝
        </BaseButton>
      </div>

      <ComputerDiagnosticsBanner
        :computer="computer"
        :busy="computerBusy"
        :troubleshooting="screenPermissionTroubleshooting"
        @request-accessibility="requestAccessibilityPermission"
        @request-screen="requestScreenPermission"
        @refresh="refreshComputerScreen"
      />

      <div v-if="computer?.pendingRequestId" class="ui-inset mt-3 flex flex-wrap items-center gap-3 p-3">
        <div class="min-w-0 flex-1">
          <strong class="text-xs">
            有 AI 客户端申请{{ computer.pendingScope === "control" ? "控制指定应用" : "查看屏幕" }}
          </strong>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            请求编号：{{ computer.pendingRequestId }} · {{ computer.pendingExpiresInSeconds ?? 0 }} 秒后失效
          </p>
          <SelectField
            v-if="computer.pendingScope === 'control'"
            v-model="selectedComputerApp"
            class="mt-2"
            label="允许控制的应用（须切换该应用到前台后才能操作）"
            :options="computerAppOptions"
          />
        </div>
        <BaseButton
          :busy="computerBusy"
          :disabled="!computer.supported || (computer.pendingScope === 'control' && (!computer.accessibilityGranted || !selectedComputerApp))"
          @click="approveComputer"
        >
          {{ computer.screenRecordingGranted === false ? "批准并尝试系统截图" : "批准 5 分钟" }}
        </BaseButton>
        <BaseButton variant="secondary" :disabled="computerBusy" @click="revokeComputer">
          拒绝
        </BaseButton>
      </div>

      <div v-else class="mt-3 flex flex-wrap items-center justify-between gap-3">
        <span class="text-xs text-[var(--ui-text-muted)]">
          {{
            computer?.active
              ? `${computer.activeScope === "control" ? "控制" : "查看"}授权 · 剩余 ${computer.expiresInSeconds ?? 0} 秒${computer.targetAppBundle ? " · " + computer.targetAppBundle : ""}${computer.remainingControlActions !== null ? " · 剩余操作 " + computer.remainingControlActions + " 次" : ""}`
              : "当前没有已批准的 Computer Use 会话"
          }}
        </span>
        <div v-if="computer?.active" class="flex flex-wrap items-center gap-2">
          <BaseButton variant="secondary" :disabled="computerBusy" @click="toggleComputerPause">
            {{ computer.paused ? "恢复" : "紧急暂停" }}
          </BaseButton>
          <BaseButton variant="secondary" :disabled="computerBusy" @click="revokeComputer">
            立即撤销
          </BaseButton>
        </div>
        <BaseButton v-else variant="ghost" :disabled="computerBusy" @click="refreshComputerScreen">
          刷新
        </BaseButton>
      </div>

      <ComputerAuditPanel
        :records="computerAuditRecords"
        :health-message="computerAuditHealth"
        :busy="auditBusy"
        @refresh="refreshComputerAudit"
      />
    </GlassCard>
  </div>
</template>

<style scoped>
.section-heading { min-width: 0; }
.settings-two-col {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: .75rem;
  min-width: 0;
}
@container app-main (max-width: 700px) {
  .settings-two-col { grid-template-columns: 1fr; }
}
</style>
