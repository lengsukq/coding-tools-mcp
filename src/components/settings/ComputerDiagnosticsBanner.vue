<script setup lang="ts">
import BaseButton from "$src/components/ui/BaseButton.vue";
import type { ComputerScreenStatus } from "$lib/api/computer";

defineProps<{
  computer: ComputerScreenStatus | null;
  busy: boolean;
  troubleshooting: string;
}>();

const emit = defineEmits<{
  "request-accessibility": [];
  "request-screen": [];
  refresh: [];
}>();
</script>

<template>
  <div
    v-if="computer?.accessibilityGranted === false"
    class="ui-inset mt-3 flex flex-wrap items-center justify-between gap-3 p-3"
  >
    <p class="text-xs text-[var(--ui-text-muted)]">控制鼠标和键盘需要 macOS 辅助功能权限。</p>
    <BaseButton variant="secondary" :disabled="busy" @click="emit('request-accessibility')">
      请求辅助功能权限
    </BaseButton>
  </div>

  <div v-if="computer && !computer.supported" class="ui-banner ui-banner--warning mt-3 text-xs">
    首版仅支持 macOS。
  </div>

  <div
    v-else-if="computer?.screenRecordingGranted === false"
    class="ui-inset mt-3 flex flex-wrap items-start justify-between gap-3 p-3"
  >
    <div class="min-w-0 flex-1">
      <p class="text-xs font-semibold">
        {{
          computer.screenCaptureSucceededInSession
            ? "当前授权会话已通过 ScreenCaptureKit 实际截图（预检仍未通过）"
            : "当前运行进程未通过 macOS 屏幕录制权限预检"
        }}
      </p>
      <p
        v-if="computer.screenCaptureSucceededInSession"
        class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]"
      >
        本次会话已经收到真实截图，说明这次 ScreenCaptureKit 调用成功。后续每次截图仍由 macOS 独立检查权限。
      </p>
      <p class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]">
        仅预检未通过不再阻断本机授权：可先让 MCP 客户端申请 Computer Use，在这里手动批准后，由 ScreenCaptureKit 实际截图结果判断是否可用。macOS 拒绝时不会返回屏幕图片；本机批准不等于授予系统权限。
      </p>
      <p class="mt-1 text-xs leading-5 text-[var(--ui-text-muted)]">{{ troubleshooting }}</p>
      <details class="mt-2 text-xs text-[var(--ui-text-muted)]">
        <summary class="cursor-pointer">查看当前运行进程的权限诊断</summary>
        <div class="mt-2 grid gap-1 break-all">
          <p>实际 Bundle ID：{{ computer.screenPermissionDiagnostic?.bundle_id || "无法获取" }}</p>
          <p>预期 Bundle ID：{{ computer.screenPermissionDiagnostic?.expected_bundle_id || "com.codingtools.mcp.desktop" }}</p>
          <p>启动形式：{{ computer.screenPermissionDiagnostic?.is_app_bundle ? ".app 应用包" : "独立可执行文件 / 开发版本" }}</p>
          <p>应用路径：{{ computer.screenPermissionDiagnostic?.bundle_path || "无法获取" }}</p>
          <p>可执行程序：{{ computer.screenPermissionDiagnostic?.executable_path || "无法获取" }}</p>
          <p>
            代码签名：{{
              computer.screenPermissionDiagnostic?.signature_type === "ad_hoc"
                ? "Ad-hoc 临时签名（权限可能随重新构建失效）"
                : computer.screenPermissionDiagnostic?.signature_type === "certificate"
                  ? "证书签名"
                  : computer.screenPermissionDiagnostic?.signature_type === "unsigned_or_unknown"
                    ? "无证书签名 / 未知"
                    : "诊断不可用"
            }}
          </p>
          <p>签名 Team ID：{{ computer.screenPermissionDiagnostic?.signing_team_id || "未设置" }}</p>
          <p>签名标识：{{ computer.screenPermissionDiagnostic?.signing_identifier || "无法获取" }}</p>
          <p>
            ScreenCaptureKit 用途声明：{{
              computer.screenPermissionDiagnostic?.capture_usage_description_present === true
                ? "已包含"
                : computer.screenPermissionDiagnostic?.capture_usage_description_present === false
                  ? "未包含（请安装新版）"
                  : "旧版本未提供此诊断"
            }}
          </p>
          <p>当前进程检测：{{ computer.screenPermissionDiagnostic?.preflight_granted ? "通过" : "未通过" }}</p>
        </div>
      </details>
    </div>
    <div class="flex flex-wrap gap-2">
      <BaseButton variant="secondary" :disabled="busy" @click="emit('refresh')">重新检测</BaseButton>
      <BaseButton variant="secondary" :disabled="busy" @click="emit('request-screen')">请求系统权限</BaseButton>
    </div>
  </div>
</template>
