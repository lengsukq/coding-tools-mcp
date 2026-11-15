import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import {
  approveComputerScreenRequest,
  decideComputerSensitiveAction,
  enableComputerAccess,
  getComputerScreenStatus,
  listComputerAvailableApps,
  pauseComputerAccess,
  requestComputerAccessibilityPermission,
  requestComputerScreenPermission,
  resumeComputerAccess,
  revokeComputerScreenAccess,
  type ComputerApplication,
  type ComputerScreenStatus,
} from "$lib/api/computer";
import { listToolAuditRecords, type ToolAuditRecord } from "$lib/api/tool-audit";
import { showToast } from "$lib/stores/toast";

export const MANUAL_DURATION_OPTIONS = [
  { value: "5", label: "5 分钟" },
  { value: "15", label: "15 分钟" },
  { value: "60", label: "1 小时" },
  { value: "480", label: "8 小时" },
  { value: "1440", label: "24 小时" },
  { value: "0", label: "永久（直到手动关闭）" },
];

export const MANUAL_SCOPE_OPTIONS = [
  { value: "observe", label: "仅查看屏幕" },
  { value: "control", label: "控制指定应用" },
];

export function useComputerSettings() {
  const computer = ref<ComputerScreenStatus | null>(null);
  const computerBusy = ref(false);
  const computerApps = ref<ComputerApplication[]>([]);
  const selectedComputerApp = ref("");
  const manualScope = ref<"observe" | "control">("observe");
  const manualDuration = ref("15");
  const manualTargetApp = ref("");
  const confirmPermanent = ref(false);
  const computerAuditRecords = ref<ToolAuditRecord[]>([]);
  const computerAuditHealth = ref("");
  const auditBusy = ref(false);

  const computerAppOptions = computed(() =>
    computerApps.value.map((app) => ({ value: app.bundle_id, label: app.name })),
  );

  const manualExpiryLabel = computed(() => {
    if (!computer.value?.manualEnabled) return "未开启";
    if (computer.value.manualExpiresAtMs === null) return "永久开启（直到手动关闭）";
    return `截止 ${new Date(computer.value.manualExpiresAtMs).toLocaleString("zh-CN")}`;
  });

  const screenPermissionTroubleshooting = computed(() => {
    const diagnostic = computer.value?.screenPermissionDiagnostic;
    if (!diagnostic?.ok) return "当前进程的权限诊断暂不可用；请先更新并重新打开桌面应用。";
    if (!diagnostic.is_app_bundle) {
      return "检测到当前程序不是从 macOS .app 应用包运行。开发版与系统设置里的正式应用可能不是同一权限主体；请完全退出开发进程后，从「应用程序」启动已授权的正式 App。";
    }
    if (diagnostic.bundle_id !== diagnostic.expected_bundle_id) {
      return "当前运行进程的 Bundle ID 与项目正式应用不一致。请退出此版本，改从「应用程序」启动 Coding Tools MCP，并确认系统设置中授权的确实是同一份应用。";
    }
    if (diagnostic.signature_type === "ad_hoc") {
      return "已检测到当前应用使用 Ad-hoc 临时签名。这类签名的代码身份可能每次构建都会变化，macOS 旧的录屏授权无法沿用。请使用稳定的 Apple Development / Developer ID 证书重新签名构建，安装后对新签名的应用重新授权。重复安装未固定签名的 DMG 无法根治。";
    }
    if (diagnostic.signature_type === "unsigned_or_unknown") {
      return "当前应用没有可验证的证书签名身份。请先用稳定的 Apple 开发者证书签名构建，再针对新身份重新授权；仅重新打开权限开关可能无效。";
    }
    if (diagnostic.capture_usage_description_present === false) {
      return "此版本未包含 ScreenCaptureKit 的 NSScreenCaptureUsageDescription 用途声明。请安装修复后的签名应用并完全退出旧进程，再重新授权。";
    }
    return "当前应用已使用正式 Bundle ID 和证书签名，但系统权限预检仍未通过。请先批准一次 MCP Computer Use 请求并尝试真实截图；若系统实际拒绝，可在完全退出应用后运行终端命令「tccutil reset ScreenCapture com.codingtools.mcp.desktop」，只清除本应用的录屏授权，再重新打开应用、点击「请求系统权限」、允许并完整重启。切勿使用 tccutil reset All。";
  });

  async function refreshComputerAudit() {
    if (auditBusy.value) return;
    auditBusy.value = true;
    try {
      const page = await listToolAuditRecords({ search: "computer_", limit: 30 });
      computerAuditRecords.value = page.records.filter((record) =>
        record.toolName.startsWith("computer_"),
      );
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
      if (!computerApps.value.some((app) => app.bundle_id === manualTargetApp.value)) {
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
      computer.value = await enableComputerAccess(
        manualScope.value,
        duration,
        manualScope.value === "control" ? manualTargetApp.value : undefined,
      );
      confirmPermanent.value = false;
      showToast(
        "Computer Use 已由本机主动开启。已认证的 MCP 客户端现在可在授权范围内直接访问，所有调用均记录审计日志。",
        { kind: "success", duration: 6500 },
      );
      void refreshComputerAudit();
    } catch (error) {
      showToast(String(error), { title: "开启 Computer Use 失败", kind: "error" });
    } finally {
      computerBusy.value = false;
    }
  }

  async function decideSensitiveAction(approve: boolean) {
    const confirmationId = computer.value?.pendingSensitiveActionId;
    if (!confirmationId || computerBusy.value) return;
    computerBusy.value = true;
    try {
      computer.value = await decideComputerSensitiveAction(confirmationId, approve);
      showToast(
        approve
          ? "已批准一次敏感动作；AI 须在有效期内携带确认 ID 原样重试，批准本身不会执行动作。"
          : "已拒绝本次敏感操作。",
        { kind: "info" },
      );
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
      showToast(
        computer.value.paused
          ? "Computer Use 已暂停，截图和输入均已阻止。"
          : "Computer Use 已恢复，请重新截图确认当前界面。",
        { kind: "info" },
      );
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
        showToast("请在 macOS 系统设置 → 隐私与安全性 → 辅助功能中允许 Coding Tools MCP。", {
          kind: "info",
          duration: 7000,
        });
      }
    } catch (error) {
      showToast(String(error), { title: "无法请求辅助功能权限", kind: "error" });
    }
  }

  async function requestScreenPermission() {
    try {
      computer.value = await requestComputerScreenPermission();
      if (!computer.value.screenRecordingGranted) {
        showToast(
          "请在 macOS 系统设置的屏幕录制权限中授权 Coding Tools MCP；授权后可能需要重启应用。",
          { kind: "info", duration: 7000 },
        );
      }
    } catch (error) {
      showToast(String(error), { title: "无法请求屏幕录制权限", kind: "error" });
    }
  }

  async function refreshComputerScreen() {
    try {
      const previous = computer.value?.pendingRequestId;
      computer.value = await getComputerScreenStatus();
      if (
        computer.value.pendingScope === "control" &&
        computer.value.pendingRequestId &&
        computer.value.pendingRequestId !== previous
      ) {
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
      computer.value = await approveComputerScreenRequest(
        requestId,
        control ? selectedComputerApp.value : undefined,
      );
      showToast(
        preflightFailed
          ? "已批准此 MCP 会话尝试使用 ScreenCaptureKit。macOS 是否允许实际截图仍由系统判断；若失败请查看 MCP 返回的系统错误。"
          : control
            ? "已授权当前会话控制选定应用 5 分钟。"
            : "已授权当前会话查看屏幕 5 分钟。",
        { kind: preflightFailed ? "info" : "success", duration: preflightFailed ? 8500 : 4000 },
      );
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

  watch(manualScope, (scope) => {
    // Listing all running applications is only needed for control grants.
    if (scope === "control" && !computerApps.value.length) void refreshComputerApps();
  });

  let computerPoll: ReturnType<typeof setInterval> | undefined;
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

  return {
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
  };
}
