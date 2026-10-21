<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import StatusPill from "$src/components/ui/StatusPill.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import { showToast } from "$lib/stores/toast";
import {
  approveComputerScreenRequest,
  enableComputerAccess,
  decideComputerSensitiveAction,
  getComputerScreenStatus,
  listComputerAvailableApps,
  pauseComputerAccess,
  requestComputerAccessibilityPermission,
  requestComputerScreenPermission,
  resumeComputerAccess,
  revokeComputerScreenAccess,
  type ComputerScreenStatus,
  type ComputerApplication,
} from "$lib/api/computer";
import { listToolAuditRecords, type ToolAuditRecord } from "$lib/api/tool-audit";


const computer = ref<ComputerScreenStatus | null>(null);
const computerBusy = ref(false);
const computerApps = ref<ComputerApplication[]>([]);
const selectedComputerApp = ref("");
const computerAppOptions = computed(() => computerApps.value.map((app) => ({ value: app.bundle_id, label: app.name })));
const manualScope = ref<"observe" | "control">("observe");
const manualDuration = ref("15");
const manualTargetApp = ref("");
const confirmPermanent = ref(false);
const computerAuditRecords = ref<ToolAuditRecord[]>([]);
const computerAuditHealth = ref("");
const auditBusy = ref(false);
const manualDurationOptions = [
  { value: "5", label: "5 分钟" },
  { value: "15", label: "15 分钟" },
  { value: "60", label: "1 小时" },
  { value: "480", label: "8 小时" },
  { value: "1440", label: "24 小时" },
  { value: "0", label: "永久（直到手动关闭）" },
];
const manualScopeOptions = [
  { value: "observe", label: "仅查看屏幕" },
  { value: "control", label: "控制指定应用" },
];
const manualExpiryLabel = computed(() => {
  if (!computer.value?.manualEnabled) return "未开启";
  if (computer.value.manualExpiresAtMs === null) return "永久开启（直到手动关闭）";
  return `截止 ${new Date(computer.value.manualExpiresAtMs).toLocaleString("zh-CN")}`;
});
function auditTime(timestampMs: number) {
  return new Date(timestampMs).toLocaleString("zh-CN", { hour12: false });
}
async function refreshComputerAudit() {
  if (auditBusy.value) return;
  auditBusy.value = true;
  try {
    const page = await listToolAuditRecords({ search: "computer_", limit: 30 });
    computerAuditRecords.value = page.records.filter((record) => record.toolName.startsWith("computer_"));
    computerAuditHealth.value = page.healthMessage ?? "";
  } catch (error) {
    computerAuditHealth.value = String(error);
  } finally {
    auditBusy.value = false;
  }
}
async function refreshComputerApps() {
  try {
    const apps = await listComputerAvailableApps();
    computerApps.value = apps.applications ?? [];
    if (!computerApps.value.some((a) => a.bundle_id === manualTargetApp.value)) {
      manualTargetApp.value = "";
    }
  } catch (error) {
    showToast(String(error), { title: "获取应用列表失败", kind: "error" });
  }
}
async function enableManualComputer() {
  if (computerBusy.value) return;
  const duration = Number(manualDuration.value);
  if (duration === 0 && !confirmPermanent.value) {
    showToast("永久授权需要勾选确认选项", { title: "请确认授权范围", kind: "error" });
    return;
  }
  if (manualScope.value === "control" && !manualTargetApp.value) {
    showToast("控制权限必须选择目标应用", { kind: "error" });
    return;
  }
  computerBusy.value = true;
  try {
    computer.value = await enableComputerAccess(manualScope.value, duration,
      manualScope.value === "control" ? manualTargetApp.value : undefined);
    confirmPermanent.value = false;
    showToast("Computer Use 已由本机主动开启。已认证的 MCP 客户端现在可在授权范围内直接访问，所有调用均记录审计日志。", { kind: "success", duration: 6500 });
    void refreshComputerAudit();
  } catch (error) {
    showToast(String(error), { title: "开启 Computer Use 失败", kind: "error" });
  } finally {
    computerBusy.value = false;
  }
}
const screenPermissionTroubleshooting = computed(() => {
  const diagnostic = computer.value?.screenPermissionDiagnostic;
  if (!diagnostic?.ok) return "当前进程的权限诊断暂不可用；请先更新并重新打开桌面应用。";
  if (!diagnostic.is_app_bundle) return "检测到当前程序不是从 macOS .app 应用包运行。开发版与系统设置里的正式应用可能不是同一权限主体；请完全退出开发进程后，从「应用程序」启动已授权的正式 App。";
  if (diagnostic.bundle_id !== diagnostic.expected_bundle_id) return "当前运行进程的 Bundle ID 与项目正式应用不一致。请退出此版本，改从「应用程序」启动 Coding Tools MCP，并确认系统设置中授权的确实是同一份应用。";
  if (diagnostic.signature_type === "ad_hoc")
    return "已检测到当前应用使用 Ad-hoc 临时签名。这类签名的代码身份可能每次构建都会变化，macOS 旧的录屏授权无法沿用。请使用稳定的 Apple Development / Developer ID 证书重新签名构建，安装后对新签名的应用重新授权。重复安装未固定签名的 DMG 无法根治。";
  if (diagnostic.signature_type === "unsigned_or_unknown")
    return "当前应用没有可验证的证书签名身份。请先用稳定的 Apple 开发者证书签名构建，再针对新身份重新授权；仅重新打开权限开关可能无效。";
  if (diagnostic.capture_usage_description_present === false)
    return "此版本未包含 ScreenCaptureKit 的 NSScreenCaptureUsageDescription 用途声明。请安装修复后的签名应用并完全退出旧进程，再重新授权。";
  return "当前应用已使用正式 Bundle ID 和证书签名，但系统权限预检仍未通过。请先批准一次 MCP Computer Use 请求并尝试真实截图；若系统实际拒绝，可在完全退出应用后运行终端命令「tccutil reset ScreenCapture com.codingtools.mcp.desktop」，只清除本应用的录屏授权，再重新打开应用、点击「请求系统权限」、允许并完整重启。切勿使用 tccutil reset All。";
});
let computerPoll: ReturnType<typeof setInterval> | undefined;
watch(manualScope, scope => {
  // Listing all running applications is only needed for control grants.
  if (scope === "control" && !computerApps.value.length) void refreshComputerApps();
});

async function decideSensitiveAction(approve: boolean) {
  const confirmationId = computer.value?.pendingSensitiveActionId;
  if (!confirmationId || computerBusy.value) return;
  computerBusy.value = true;
  try {
    computer.value = await decideComputerSensitiveAction(confirmationId, approve);
    showToast(approve
      ? "已批准一次敏感动作；AI 须在有效期内携带确认 ID 原样重试，批准本身不会执行动作。"
      : "已拒绝本次敏感操作。",
    { kind: "info" });
  } catch (error) {
    showToast(String(error), { title: "敏感操作审批失败", kind: "error" });
  } finally {
    computerBusy.value = false;
  }
}

async function toggleComputerPause() {
  if (computerBusy.value || !computer.value?.active) return;
  computerBusy.value = true;
  try {
    computer.value = computer.value.paused
      ? await resumeComputerAccess()
      : await pauseComputerAccess();
    showToast(computer.value.paused ? "Computer Use 已暂停，截图和输入均已阻止。" : "Computer Use 已恢复，请重新截图确认当前界面。", { kind: "info" });
  } catch (error) {
    showToast(String(error), { title: "切换 Computer Use 状态失败", kind: "error" });
  } finally {
    computerBusy.value = false;
  }
}

async function requestAccessibilityPermission() {
  try {
    computer.value = await requestComputerAccessibilityPermission();
    if (!computer.value.accessibilityGranted) {
      showToast("请在 macOS 系统设置 → 隐私与安全性 → 辅助功能中允许 Coding Tools MCP。", { kind: "info", duration: 7000 });
    }
  } catch (error) {
    showToast(String(error), { title: "无法请求辅助功能权限", kind: "error" });
  }
}

async function requestScreenPermission() {
  try {
    computer.value = await requestComputerScreenPermission();
    if (!computer.value.screenRecordingGranted) {
      showToast("请在 macOS 系统设置的屏幕录制权限中授权 Coding Tools MCP；授权后可能需要重启应用。", { kind: "info", duration: 7000 });
    }
  } catch (error) {
    showToast(String(error), { title: "无法请求屏幕录制权限", kind: "error" });
  }
}


async function refreshComputerScreen() {
  try {
    const previous = computer.value?.pendingRequestId;
    computer.value = await getComputerScreenStatus();
    if (computer.value.pendingScope === "control" && computer.value.pendingRequestId && computer.value.pendingRequestId !== previous) {
      const result = await listComputerAvailableApps();
      computerApps.value = result.applications || [];
      selectedComputerApp.value = "";
    }
  } catch {
    // MCP connection settings must remain functional if the desktop command
    // is unavailable in an older installed runtime.
  }
}

async function approveComputer() {
  const requestId = computer.value?.pendingRequestId;
  if (!requestId || computerBusy.value) return;
  computerBusy.value = true;
  try {
    const control = computer.value?.pendingScope === "control";
    const preflightFailed = computer.value?.screenRecordingGranted === false;
    computer.value = await approveComputerScreenRequest(requestId, control ? selectedComputerApp.value : undefined);
    showToast(preflightFailed
      ? "已批准此 MCP 会话尝试使用 ScreenCaptureKit。macOS 是否允许实际截图仍由系统判断；若失败请查看 MCP 返回的系统错误。"
      : (control ? "已授权当前会话控制选定应用 5 分钟。" : "已授权当前会话查看屏幕 5 分钟。"),
    { kind: preflightFailed ? "info" : "success", duration: preflightFailed ? 8500 : 4000 });
  } catch (error) {
    showToast(String(error), { title: "屏幕授权失败", kind: "error" });
  } finally {
    computerBusy.value = false;
  }
}

async function revokeComputer() {
  if (computerBusy.value) return;
  computerBusy.value = true;
  try {
    computer.value = await revokeComputerScreenAccess();
    showToast("Computer Use 屏幕访问已关闭", { kind: "info" });
  } catch (error) {
    showToast(String(error), { title: "撤销失败", kind: "error" });
  } finally {
    computerBusy.value = false;
  }
}


onMounted(() => {
  void refreshComputerScreen();
  void refreshComputerAudit();
  // Approvals need timely updates while this page is open. Do not poll on
  // unsupported Windows hosts or while the application is in the background.
  computerPoll = setInterval(() => {
    if (document.visibilityState === "visible" && computer.value?.supported !== false) {
      void refreshComputerScreen();
    }
  }, 3000);
});
onUnmounted(() => {
  if (computerPoll) clearInterval(computerPoll);
});
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
        <StatusPill :status="computer?.active && !computer?.paused ? 'running' : 'stopped'" :label="computer?.paused ? '已暂停' : computer?.manualEnabled ? '本机主动授权' : computer?.active ? '旧会话授权' : '未授权'" />
      </div>
      <div class="ui-inset mt-4 grid gap-3 p-3">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div>
            <strong class="text-sm">本机主动授权</strong>
            <p class="mt-1 text-xs text-[var(--ui-text-muted)]">{{ manualExpiryLabel }} · {{ computer?.manualEnabled ? (computer.manualScope === "control" ? "控制指定应用" : "仅查看屏幕") : "尚未开启" }}</p>
          </div>
          <div v-if="computer?.manualEnabled" class="flex flex-wrap gap-2">
            <BaseButton variant="secondary" :disabled="computerBusy" @click="toggleComputerPause">{{ computer.manualPaused ? "恢复" : "紧急暂停" }}</BaseButton>
            <BaseButton variant="secondary" :disabled="computerBusy" @click="revokeComputer">立即关闭</BaseButton>
          </div>
        </div>
        <div class="settings-two-col">
          <SelectField v-model="manualScope" label="权限范围" :options="manualScopeOptions" />
          <SelectField v-model="manualDuration" label="授权时间" :options="manualDurationOptions" />
        </div>
        <div v-if="manualScope === 'control'" class="settings-two-col">
          <SelectField v-model="manualTargetApp" label="只允许操作此应用" :options="computerAppOptions" />
          <div class="flex items-end"><BaseButton variant="ghost" :disabled="computerBusy" @click="refreshComputerApps">刷新应用列表</BaseButton></div>
        </div>
        <p v-if="manualScope === 'control'" class="text-xs leading-5 text-[var(--ui-text-muted)]">
          控制模式仅针对所选应用；鼠标坐标仍需近期截图，删除、支付、发送等敏感操作仍需你单独确认。切换客户端会使旧截图失效。
        </p>
        <div v-if="manualDuration === '0'" class="ui-inset p-3">
          <ToggleSwitch v-model="confirmPermanent" label="我确认永久授权" description="永久授权在应用重启后继续生效，直到在这里手动关闭。只建议用于你信任的 MCP 客户端与网络环境。" />
        </div>
        <div class="flex flex-wrap gap-2">
          <BaseButton :busy="computerBusy" :disabled="!computer?.supported || computerBusy || (manualScope === 'control' && (!manualTargetApp || !computer?.accessibilityGranted)) || (manualDuration === '0' && !confirmPermanent)" @click="enableManualComputer">{{ computer?.manualEnabled ? "更新本机授权" : "开启 Computer Use" }}</BaseButton>
          <BaseButton v-if="computer?.manualEnabled" variant="ghost" :disabled="computerBusy" @click="revokeComputer">撤销全部访问</BaseButton>
        </div>
      </div>
      <div v-if="computer?.active && computer.pendingSensitiveActionId" class="ui-inset mt-3 flex flex-wrap items-center gap-3 p-3">
        <div class="min-w-0 flex-1">
          <strong class="text-xs">高风险操作需单独确认</strong>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            目标应用：{{ computer.pendingSensitiveActionApp }} · {{ computer.pendingSensitiveActionDescription }}
          </p>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            {{ computer.pendingSensitiveActionApproved ? "已允许一次，等待原客户端重试执行；不可重复使用" : "审批本身不会执行操作" }} · 剩余 {{ computer.pendingSensitiveActionExpiresInSeconds ?? 0 }} 秒
          </p>
        </div>
        <BaseButton v-if="!computer.pendingSensitiveActionApproved" :disabled="computerBusy || computer.paused" @click="decideSensitiveAction(true)">允许本次</BaseButton>
        <BaseButton variant="secondary" :disabled="computerBusy" @click="decideSensitiveAction(false)">拒绝</BaseButton>
      </div>
      <div v-if="computer?.accessibilityGranted === false" class="ui-inset mt-3 flex flex-wrap items-center justify-between gap-3 p-3">
        <p class="text-xs text-[var(--ui-text-muted)]">控制鼠标和键盘需要 macOS 辅助功能权限。</p>
        <BaseButton variant="secondary" :disabled="computerBusy" @click="requestAccessibilityPermission">请求辅助功能权限</BaseButton>
      </div>
      <div v-if="computer && !computer.supported" class="ui-banner ui-banner--warning mt-3 text-xs">
        首版仅支持 macOS。
      </div>
      <div v-else-if="computer?.screenRecordingGranted === false" class="ui-inset mt-3 flex flex-wrap items-start justify-between gap-3 p-3">
        <div class="min-w-0 flex-1">
          <p class="text-xs font-semibold">{{ computer.screenCaptureSucceededInSession ? "当前授权会话已通过 ScreenCaptureKit 实际截图（预检仍未通过）" : "当前运行进程未通过 macOS 屏幕录制权限预检" }}</p>
          <p v-if="computer.screenCaptureSucceededInSession" class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]">本次会话已经收到真实截图，说明这次 ScreenCaptureKit 调用成功。后续每次截图仍由 macOS 独立检查权限。</p>
          <p class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]">仅预检未通过不再阻断本机授权：可先让 MCP 客户端申请 Computer Use，在这里手动批准后，由 ScreenCaptureKit 实际截图结果判断是否可用。macOS 拒绝时不会返回屏幕图片；本机批准不等于授予系统权限。</p>
          <p class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]">{{ screenPermissionTroubleshooting }}</p>
          <details class="mt-2 text-xs text-[var(--ui-text-muted)]">
            <summary class="cursor-pointer">查看当前运行进程的权限诊断</summary>
            <div class="mt-2 grid gap-1 break-all">
              <p>实际 Bundle ID：{{ computer.screenPermissionDiagnostic?.bundle_id || "无法获取" }}</p>
              <p>预期 Bundle ID：{{ computer.screenPermissionDiagnostic?.expected_bundle_id || "com.codingtools.mcp.desktop" }}</p>
              <p>启动形式：{{ computer.screenPermissionDiagnostic?.is_app_bundle ? ".app 应用包" : "独立可执行文件 / 开发版本" }}</p>
              <p>应用路径：{{ computer.screenPermissionDiagnostic?.bundle_path || "无法获取" }}</p>
              <p>可执行程序：{{ computer.screenPermissionDiagnostic?.executable_path || "无法获取" }}</p>
              <p>代码签名：{{ computer.screenPermissionDiagnostic?.signature_type === "ad_hoc" ? "Ad-hoc 临时签名（权限可能随重新构建失效）" : computer.screenPermissionDiagnostic?.signature_type === "certificate" ? "证书签名" : computer.screenPermissionDiagnostic?.signature_type === "unsigned_or_unknown" ? "无证书签名 / 未知" : "诊断不可用" }}</p>
              <p>签名 Team ID：{{ computer.screenPermissionDiagnostic?.signing_team_id || "未设置" }}</p>
              <p>签名标识：{{ computer.screenPermissionDiagnostic?.signing_identifier || "无法获取" }}</p>
              <p>ScreenCaptureKit 用途声明：{{ computer.screenPermissionDiagnostic?.capture_usage_description_present === true ? "已包含" : computer.screenPermissionDiagnostic?.capture_usage_description_present === false ? "未包含（请安装新版）" : "旧版本未提供此诊断" }}</p>
              <p>当前进程检测：{{ computer.screenPermissionDiagnostic?.preflight_granted ? "通过" : "未通过" }}</p>
            </div>
          </details>
        </div>
        <div class="flex flex-wrap gap-2">
          <BaseButton variant="secondary" :disabled="computerBusy" @click="refreshComputerScreen">重新检测</BaseButton>
          <BaseButton variant="secondary" :disabled="computerBusy" @click="requestScreenPermission">请求系统权限</BaseButton>
        </div>
      </div>
      <div v-if="computer?.pendingRequestId" class="ui-inset mt-3 flex flex-wrap items-center gap-3 p-3">
        <div class="min-w-0 flex-1">
          <strong class="text-xs">有 AI 客户端申请{{ computer.pendingScope === "control" ? "控制指定应用" : "查看屏幕" }}</strong>
          <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
            请求编号：{{ computer.pendingRequestId }} · {{ computer.pendingExpiresInSeconds ?? 0 }} 秒后失效
          </p>
          <SelectField v-if="computer.pendingScope === 'control'" v-model="selectedComputerApp" class="mt-2" label="允许控制的应用（须切换该应用到前台后才能操作）" :options="computerAppOptions" />
        </div>
        <BaseButton :busy="computerBusy" :disabled="!computer.supported || (computer.pendingScope === 'control' && (!computer.accessibilityGranted || !selectedComputerApp))" @click="approveComputer">{{ computer.screenRecordingGranted === false ? "批准并尝试系统截图" : "批准 5 分钟" }}</BaseButton>
        <BaseButton variant="secondary" :disabled="computerBusy" @click="revokeComputer">拒绝</BaseButton>
      </div>
      <div v-else class="mt-3 flex flex-wrap items-center justify-between gap-3">
        <span class="text-xs text-[var(--ui-text-muted)]">
          {{ computer?.active ? `${computer.activeScope === "control" ? "控制" : "查看"}授权 · 剩余 ${computer.expiresInSeconds ?? 0} 秒${computer.targetAppBundle ? " · " + computer.targetAppBundle : ""}${computer.remainingControlActions !== null ? " · 剩余操作 " + computer.remainingControlActions + " 次" : ""}` : "当前没有已批准的 Computer Use 会话" }}
        </span>
        <div v-if="computer?.active" class="flex flex-wrap items-center gap-2">
          <BaseButton variant="secondary" :disabled="computerBusy" @click="toggleComputerPause">{{ computer.paused ? "恢复" : "紧急暂停" }}</BaseButton>
          <BaseButton variant="secondary" :disabled="computerBusy" @click="revokeComputer">立即撤销</BaseButton>
        </div>
        <BaseButton v-else variant="ghost" :disabled="computerBusy" @click="refreshComputerScreen">刷新</BaseButton>
      </div>
      <div class="mt-5 border-t border-[var(--ui-line)] pt-4">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <div>
            <h3 class="text-sm font-semibold">Computer Use 审计日志</h3>
            <p class="mt-1 text-xs text-[var(--ui-text-muted)]">显示最近 30 次请求。记录认证方式、OAuth 客户端 ID、时间、操作类型、结果；不保存屏幕图片、键盘输入内容或授权令牌。</p>
          </div>
          <BaseButton variant="ghost" :busy="auditBusy" @click="refreshComputerAudit">刷新日志</BaseButton>
        </div>
        <p v-if="computerAuditHealth" class="ui-danger mt-2 text-xs" role="alert">{{ computerAuditHealth }}</p>
        <div class="mt-3 max-h-72 space-y-2 overflow-y-auto">
          <div v-for="(record, index) in computerAuditRecords" :key="record.requestId || index" class="ui-inset grid gap-1 p-3 text-xs">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <strong>{{ record.toolName }}{{ record.actionName ? ' / ' + record.actionName : '' }}</strong>
              <span :class="record.outcome === 'success' ? 'text-[var(--ui-success)]' : 'text-[var(--ui-danger)]'">{{ record.outcome === "success" ? "成功" : "失败" }}</span>
            </div>
            <p class="break-all text-[var(--ui-text-muted)]">{{ auditTime(record.timestampMs) }} · {{ record.authMethod }} · 客户端：{{ record.clientId || (record.authMethod === "shared_bearer" ? "共享 Bearer 凭证（无法区分具体客户端）" : "未提供客户端 ID") }}</p>
            <p v-if="record.errorCategory" class="text-[var(--ui-text-muted)]">错误类型：{{ record.errorCategory }}</p>
          </div>
          <p v-if="!computerAuditRecords.length" class="text-xs text-[var(--ui-text-muted)]">暂无 Computer Use 调用记录</p>
        </div>
      </div>
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
