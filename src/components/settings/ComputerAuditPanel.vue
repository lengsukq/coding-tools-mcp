<script setup lang="ts">
import BaseButton from "$src/components/ui/BaseButton.vue";
import type { ToolAuditRecord } from "$lib/api/tool-audit";
import { formatAuditAuthLabel, formatAuditErrorLabel } from "$lib/audit/use-tool-audit";
import { formatFullDateTime } from "$lib/formatters";

defineProps<{
  records: ToolAuditRecord[];
  healthMessage: string;
  busy: boolean;
}>();

const emit = defineEmits<{
  refresh: [];
}>();
</script>

<template>
  <div class="mt-5 border-t border-[var(--ui-line)] pt-4">
    <div class="flex flex-wrap items-center justify-between gap-2">
      <div>
        <h3 class="text-sm font-semibold">Computer Use 审计日志</h3>
        <p class="mt-1 text-xs text-[var(--ui-text-muted)]">
          显示最近 30 次请求。记录认证方式、OAuth 客户端 ID、时间、操作类型、结果；不保存屏幕图片、键盘输入内容或授权令牌。
        </p>
      </div>
      <BaseButton variant="ghost" :busy="busy" @click="emit('refresh')">刷新日志</BaseButton>
    </div>
    <p v-if="healthMessage" class="ui-danger mt-2 text-xs" role="alert">{{ healthMessage }}</p>
    <div class="mt-3 max-h-72 space-y-2 overflow-y-auto">
      <div
        v-for="(record, index) in records"
        :key="record.requestId || index"
        class="ui-inset grid gap-1 p-3 text-xs"
      >
        <div class="flex flex-wrap items-center justify-between gap-2">
          <strong>{{ record.toolName }}{{ record.actionName ? " / " + record.actionName : "" }}</strong>
          <span :class="record.outcome === 'success' ? 'text-[var(--ui-success)]' : 'text-[var(--ui-danger)]'">
            {{ record.outcome === "success" ? "成功" : "失败" }}
          </span>
        </div>
        <p class="break-all text-[var(--ui-text-muted)]">
          {{ formatFullDateTime(record.timestampMs) }} · {{ formatAuditAuthLabel(record).text }} · 客户端：{{
            record.clientId ||
            (record.authMethod === "shared_bearer"
              ? "共享 Bearer 凭证（无法区分具体客户端）"
              : "未提供客户端 ID")
          }}
        </p>
        <p v-if="record.errorCategory" class="text-[var(--ui-text-muted)]">
          错误类型：{{ formatAuditErrorLabel(record.errorCategory) }}
        </p>
      </div>
      <p v-if="!records.length" class="text-xs text-[var(--ui-text-muted)]">
        暂无 Computer Use 调用记录
      </p>
    </div>
  </div>
</template>
