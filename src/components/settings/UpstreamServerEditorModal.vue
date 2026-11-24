<script setup lang="ts">
import { reactive, watch } from "vue";
import KeyValueSecretEditor from "$src/components/settings/KeyValueSecretEditor.vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import ModalDialog from "$src/components/ui/ModalDialog.vue";
import SegmentedControl from "$src/components/ui/SegmentedControl.vue";
import TextAreaField from "$src/components/ui/TextAreaField.vue";
import TextField from "$src/components/ui/TextField.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import {
  createEmptyUpstreamDraft,
  type UpstreamServerDraft,
} from "$lib/upstream-mcp/use-upstream-mcp";
import type { UpstreamExposeMode } from "$lib/api/upstream-mcp";

const props = defineProps<{
  open: boolean;
  initialDraft: UpstreamServerDraft;
  saving: boolean;
  error: string;
}>();

const emit = defineEmits<{
  close: [];
  submit: [draft: UpstreamServerDraft];
}>();

const form = reactive<UpstreamServerDraft>(createEmptyUpstreamDraft());

const TRANSPORT_ITEMS = [
  { value: "stdio", label: "Stdio 本地进程" },
  { value: "streamable_http", label: "Streamable HTTP" },
  { value: "sse", label: "Legacy SSE" },
];

const EXPOSE_ITEMS = [
  { value: "direct", label: "直出（{alias}__*）" },
  { value: "proxy", label: "懒加载代理（按需搜索）" },
];

watch(
  () => [props.open, props.initialDraft] as const,
  ([open, source]) => {
    if (!open) return;
    Object.assign(form, {
      ...source,
      envRows: source.envRows.map((r) => ({ ...r })),
      headerRows: source.headerRows.map((r) => ({ ...r })),
      disabledTools: [...source.disabledTools],
      originalSecretEnvKeys: [...source.originalSecretEnvKeys],
      originalSecretHeaderKeys: [...source.originalSecretHeaderKeys],
    });
  },
  { immediate: true },
);

function handleSubmit() {
  emit("submit", {
    ...form,
    envRows: form.envRows.map((r) => ({ ...r })),
    headerRows: form.headerRows.map((r) => ({ ...r })),
    disabledTools: [...form.disabledTools],
    originalSecretEnvKeys: [...form.originalSecretEnvKeys],
    originalSecretHeaderKeys: [...form.originalSecretHeaderKeys],
  });
}
</script>

<template>
  <ModalDialog
    :open="open"
    :title="form.id ? '编辑上游 MCP 服务' : '添加上游 MCP 服务'"
    description="支持挂载本机 Stdio 子进程或远程 Streamable HTTP / Legacy SSE 服务；敏感凭据将隔离保存在应用密钥区。"
    :dismissible="!saving"
    @close="emit('close')"
  >
    <form class="max-h-[70vh] space-y-4 overflow-y-auto pr-1" @submit.prevent="handleSubmit">
      <div
        v-if="error"
        role="alert"
        class="rounded-[var(--ui-radius-control)] border border-[var(--ui-danger)]/30 bg-[var(--ui-danger-soft)] px-3 py-2 text-xs text-[var(--ui-danger)]"
      >
        {{ error }}
      </div>

      <div class="grid gap-3 sm:grid-cols-2">
        <TextField
          v-model="form.alias"
          label="前缀别名 (Alias)"
          placeholder="例如 github / postgres / playwright"
          hint="仅小写字母、数字、_ 或 -（≤24 字符），用于直出工具名前缀"
          :disabled="saving"
        />
        <TextField
          v-model="form.name"
          label="显示名称"
          placeholder="例如 GitHub MCP"
          hint="用于界面展示与工具说明标注"
          :disabled="saving"
        />
      </div>

      <TextField
        v-model="form.description"
        label="用途说明（可选）"
        placeholder="帮助 AI 在懒加载代理模式下判断何时调用此服务"
        :disabled="saving"
      />

      <div class="space-y-1.5">
        <span class="block text-xs font-medium text-[var(--ui-text-secondary)]">暴露模式</span>
        <SegmentedControl
          :items="EXPOSE_ITEMS"
          :model-value="form.exposeMode"
          :disabled="saving"
          aria-label="选择工具暴露模式"
          @update:model-value="form.exposeMode = $event as UpstreamExposeMode"
        />
      </div>

      <div class="space-y-1.5">
        <span class="block text-xs font-medium text-[var(--ui-text-secondary)]">传输协议</span>
        <SegmentedControl
          :items="TRANSPORT_ITEMS"
          :model-value="form.transportType"
          :disabled="saving"
          aria-label="选择上游传输协议"
          @update:model-value="form.transportType = $event as 'stdio' | 'streamable_http' | 'sse'"
        />
      </div>

      <!-- Stdio Fields -->
      <template v-if="form.transportType === 'stdio'">
        <TextField
          v-model="form.command"
          label="启动命令"
          placeholder="例如 npx / uvx / /usr/local/bin/my-mcp"
          hint="会自动继承「全局可执行路径 PATH」，无需手写完整 Homebrew 路径"
          :disabled="saving"
        />
        <TextAreaField
          v-model="form.argsText"
          label="命令参数（每行一个参数）"
          placeholder="-y&#10;@modelcontextprotocol/server-github"
          :rows="3"
          :disabled="saving"
        />
        <TextField
          v-model="form.cwd"
          label="工作目录（可选）"
          placeholder="留空则使用默认目录"
          :disabled="saving"
        />

        <KeyValueSecretEditor
          title="环境变量 (Env)"
          add-label="新增变量"
          empty-hint="暂无环境变量。勾选锁形按钮可将 Token / API Key 加密写入 SecretStore。"
          key-placeholder="KEY"
          value-placeholder="VALUE"
          delete-aria-label="删除变量"
          secret-title-on="已标记为加密凭据（存入 SecretStore）"
          secret-title-off="点击标记为加密凭据"
          :rows="form.envRows"
          :disabled="saving"
        />
      </template>

      <!-- HTTP / SSE Fields -->
      <template v-else>
        <TextField
          v-model="form.url"
          :label="form.transportType === 'sse' ? 'Legacy SSE 端点 URL' : 'Streamable HTTP 端点 URL'"
          :placeholder="form.transportType === 'sse' ? 'https://example.com/sse' : 'https://example.com/mcp'"
          :disabled="saving"
        />

        <div class="space-y-1.5">
          <TextField
            v-model="form.bearerTokenInput"
            type="password"
            label="Bearer Token（可选，存入 SecretStore）"
            :placeholder="form.hasStoredBearer && !form.clearBearerToken ? '已加密保存（留空保持不变）' : '输入访问令牌'"
            :disabled="saving"
          />
          <div v-if="form.hasStoredBearer" class="flex items-center justify-between text-xs text-[var(--ui-text-muted)]">
            <span>{{ form.clearBearerToken ? '保存后将清除已存储的 Bearer Token' : '当前已安全存储 Bearer Token' }}</span>
            <button
              type="button"
              class="text-[var(--ui-danger)] hover:underline"
              @click="form.clearBearerToken = !form.clearBearerToken"
            >
              {{ form.clearBearerToken ? '撤销清除' : '清除现有 Token' }}
            </button>
          </div>
        </div>

        <ToggleSwitch
          v-model="form.useProxy"
          label="使用全局网络代理"
          description="访问海外或受限远程 MCP 时，遵循「网络与工具组件 → 网络代理」配置"
          :disabled="saving"
        />

        <KeyValueSecretEditor
          title="自定义 HTTP Headers"
          add-label="新增 Header"
          empty-hint="可添加 X-API-Key 等请求头；点亮锁图标即可加密存储。"
          key-placeholder="Header-Name"
          value-placeholder="Value"
          delete-aria-label="删除 Header"
          :rows="form.headerRows"
          :disabled="saving"
        />
      </template>

      <div class="grid gap-3 sm:grid-cols-2">
        <TextField
          v-model="form.callTimeoutSecs"
          type="number"
          label="调用超时（秒）"
          hint="单次 tools/call 超时（5~600，默认 60）"
          :disabled="saving"
        />
        <TextField
          v-if="form.transportType === 'stdio'"
          v-model="form.idleTimeoutSecs"
          type="number"
          label="空闲回收（秒）"
          hint="超时未调用自动结束子进程（0=常驻，默认 900）"
          :disabled="saving"
        />
      </div>

      <div class="flex items-center justify-end gap-2 border-t border-[var(--ui-line)] pt-4">
        <BaseButton variant="secondary" :disabled="saving" @click="emit('close')">取消</BaseButton>
        <BaseButton type="submit" variant="primary" :busy="saving">
          {{ form.id ? '保存修改' : '添加服务' }}
        </BaseButton>
      </div>
    </form>
  </ModalDialog>
</template>
