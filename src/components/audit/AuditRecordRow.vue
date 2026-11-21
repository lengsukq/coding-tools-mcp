<script setup lang="ts">
import { Check, AlertCircle, Zap, ArrowUp, ArrowDown, ChevronDown, Layers, Sparkles, Filter, Copy } from "@lucide/vue";
import type { ToolAuditRecord } from "$lib/api/tool-audit";

defineProps<{
  record: ToolAuditRecord;
  expanded: boolean;
  copiedKey: string | null;
  workspaceName: string;
  formatters: {
    errorLabel: (category: string | null) => string;
    formatBytes: (bytes: number | null | undefined) => string;
    formatTime: (timestampMs: number) => string;
    formatRelativeTime: (timestampMs: number) => string;
    formatDuration: (durationMs: number) => string;
    authLabel: (record: ToolAuditRecord) => { text: string; kind: "oauth" | "bearer" | "none" };
  };
}>();
const emit = defineEmits<{
  toggle: [];
  "filter-session": [sessionId: string];
  "filter-tool": [toolName: string];
  "filter-workspace": [workspaceId: string];
  "copy-text": [text: string, key: string, label: string];
}>();
</script>

<template>
<article
            class="group px-4 py-3.5 transition-colors hover:bg-[var(--ui-surface-hover)] lg:px-5 cursor-pointer"
            @click="emit('toggle')"
          >
            <!-- Primary Row -->
            <div role="button" tabindex="0" :aria-expanded="expanded" :aria-label="`${record.toolName} · 查看调用详情`" @keydown.enter.prevent="emit('toggle')" @keydown.space.prevent="emit('toggle')" class="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
              <!-- Left: Status + Tool + Wrapper + Tags -->
              <div class="flex flex-wrap items-center gap-2.5 min-w-0">
                <!-- Status icon badge -->
                <div
                  class="flex h-6 w-6 shrink-0 items-center justify-center rounded-full"
                  :class="record.outcome === 'success'
                    ? 'bg-[var(--ui-success-soft)] text-[var(--ui-success)] '
                    : 'bg-[var(--ui-danger-soft)] text-[var(--ui-danger)] '"
                >
                  <Check v-if="record.outcome === 'success'" :size="13" stroke-width="2.5" />
                  <AlertCircle v-else :size="13" stroke-width="2.5" />
                </div>

                <!-- Tool Name Badge -->
                <span class="rounded-md bg-[var(--ui-accent-soft)] px-2 py-0.5 font-mono text-xs font-semibold text-[var(--ui-accent)]">
                  {{ record.toolName }}
                </span>

                <!-- Wrapper indicator -->
                <span v-if="record.wrapperToolName" class="font-mono text-xs text-[var(--ui-text-muted)]">
                  via {{ record.wrapperToolName }}
                </span>

                <!-- Error Category Pill -->
                <span
                  v-if="record.outcome !== 'success'"
                  class="inline-flex items-center gap-1 rounded-full bg-[var(--ui-danger-soft)] px-2 py-0.5 text-xs font-semibold text-[var(--ui-danger)] "
                >
                  {{ formatters.errorLabel(record.errorCategory) }}
                </span>

                <!-- Change impact pill -->
                <span
                  v-if="record.changeId"
                  class="inline-flex items-center gap-1 rounded-full bg-[var(--ui-accent-soft)] px-2 py-0.5 text-xs font-medium text-[var(--ui-accent)] "
                  title="引发文件变更"
                >
                  <Zap :size="10" />
                  变更 {{ record.changedFileCount ?? 0 }} 文件
                </span>

                <!-- Cross-device federation pill -->
                <span
                  v-if="record.executionHost && record.executionHost !== 'local'"
                  class="inline-flex items-center gap-1 rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-2 py-0.5 text-xs text-[var(--ui-text-secondary)]"
                  title="通过局域网联邦单跳转发至配对电脑执行"
                >
                  转发 → {{ record.executionHost.replace(/^peer:/, '') }}
                </span>
                <span
                  v-else-if="record.sourcePeer"
                  class="inline-flex items-center gap-1 rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] px-2 py-0.5 text-xs text-[var(--ui-text-secondary)]"
                  title="来自局域网配对电脑发起的远端调用"
                >
                  来自 ← {{ record.sourcePeer }}
                </span>
              </div>

              <!-- Right: Duration + Time + Payload + Expand Arrow -->
              <div class="ml-auto flex shrink-0 items-center gap-3 text-xs text-[var(--ui-text-muted)]">
                <!-- Payload size -->
                <div class="hidden items-center gap-2 font-mono text-xs text-[var(--ui-text-secondary)] sm:flex">
                  <span class="inline-flex items-center gap-0.5" title="请求体积">
                    <ArrowUp :size="10" class="text-[var(--ui-accent)]" /> {{ formatters.formatBytes(record.requestBytes) }}
                  </span>
                  <span class="inline-flex items-center gap-0.5" title="响应体积">
                    <ArrowDown :size="10" class="text-[var(--ui-success)]" /> {{ formatters.formatBytes(record.responseBytes) }}
                  </span>
                </div>

                <!-- Execution Duration -->
                <span
                  class="inline-flex items-center rounded-md px-1.5 py-0.5 font-mono text-xs font-medium"
                  :class="record.durationMs < 200
                    ? 'bg-[var(--ui-surface-subtle)] text-[var(--ui-text-secondary)]'
                    : record.durationMs < 1000
                      ? 'bg-[var(--ui-accent-soft)] text-[var(--ui-accent)]'
                      : 'bg-[var(--ui-warning-soft)] text-[var(--ui-warning)]'"
                >
                  {{ formatters.formatDuration(record.durationMs) }}
                </span>

                <!-- Relative time -->
                <time
                  class="min-w-[50px] text-right text-xs text-[var(--ui-text-muted)]"
                  :title="formatters.formatTime(record.timestampMs)"
                >
                  {{ formatters.formatRelativeTime(record.timestampMs) }}
                </time>

                <!-- Expand Chevron -->
                <div
                  class="flex h-5 w-5 items-center justify-center rounded text-[var(--ui-text-muted)] transition-transform duration-200"
                  :class="{ 'rotate-180': expanded }"
                >
                  <ChevronDown :size="14" />
                </div>
              </div>
            </div>

            <!-- Secondary Subline: Workspace + Auth + Brief IDs -->
            <div class="mt-2 flex flex-wrap items-center gap-x-4 gap-y-1 pl-8.5 text-xs text-[var(--ui-text-muted)]">
              <span class="flex items-center gap-1 font-medium text-[var(--ui-text-secondary)]">
                <Layers :size="11" class="text-[var(--ui-text-muted)]" />
                {{ workspaceName }}
              </span>

              <span>•</span>

              <span>{{ formatters.authLabel(record).text }}</span>

              <template v-if="record.requestId">
                <span>•</span>
                <span class="max-w-[200px] truncate font-mono text-xs text-[var(--ui-text-muted)]" :title="record.requestId">
                  Req: {{ record.requestId.slice(0, 14) }}…
                </span>
              </template>

              <template v-if="record.sessionId">
                <span>•</span>
                <span class="max-w-[180px] truncate font-mono text-xs text-[var(--ui-text-muted)]" :title="record.sessionId">
                  Sess: {{ record.sessionId.slice(0, 12) }}…
                </span>
              </template>
            </div>

            <!-- Expanded Details Drawer -->
            <transition
              enter-active-class="transition-[opacity,transform] duration-200 ease-out"
              enter-from-class="opacity-0 -translate-y-1"
              enter-to-class="opacity-100 translate-y-0"
              leave-active-class="transition-[opacity,transform] duration-150 ease-in"
              leave-from-class="opacity-100 translate-y-0"
              leave-to-class="opacity-0 -translate-y-1"
            >
              <div
                v-if="expanded"
                class="mt-3.5 cursor-default space-y-3 rounded-xl border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] p-4 text-xs"
                @click.stop
              >
                <!-- Detail Header / Quick Filter actions -->
                <div class="flex flex-wrap items-center justify-between gap-2 border-b border-[var(--ui-line)] pb-2.5">
                  <span class="flex items-center gap-1.5 text-xs font-semibold text-[var(--ui-text)]">
                    <Sparkles :size="13" class="text-[var(--ui-accent)]" /> 调用上下文与排查诊断
                  </span>

                  <!-- Quick Filter Actions -->
                  <div class="flex flex-wrap items-center gap-1.5">
                    <button
                      v-if="record.sessionId"
                      type="button"
                      class="inline-flex items-center gap-1 rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface)] px-2.5 py-1 text-xs font-medium text-[var(--ui-text-secondary)] shadow-xs transition hover:border-[var(--ui-accent)] hover:text-[var(--ui-accent)]"
                      @click="emit('filter-session', record.sessionId)"
                    >
                      <Filter :size="10" /> 仅看此 Session
                    </button>

                    <button
                      type="button"
                      class="inline-flex items-center gap-1 rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface)] px-2.5 py-1 text-xs font-medium text-[var(--ui-text-secondary)] shadow-xs transition hover:border-[var(--ui-accent)] hover:text-[var(--ui-accent)]"
                      @click="emit('filter-tool', record.toolName)"
                    >
                      <Filter :size="10" /> 仅看此工具
                    </button>

                    <button
                      v-if="record.workspaceId"
                      type="button"
                      class="inline-flex items-center gap-1 rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface)] px-2.5 py-1 text-xs font-medium text-[var(--ui-text-secondary)] shadow-xs transition hover:border-[var(--ui-accent)] hover:text-[var(--ui-accent)]"
                      @click="emit('filter-workspace', record.workspaceId)"
                    >
                      <Filter :size="10" /> 过滤当前工作区
                    </button>
                  </div>
                </div>

                <!-- Structured Key-Value Grid -->
                <div class="grid grid-cols-1 gap-2.5 font-mono text-xs sm:grid-cols-2 lg:grid-cols-3">
                  <!-- Request ID -->
                  <div class="rounded-lg border border-[var(--ui-line)] bg-[var(--ui-surface)] p-2.5">
                    <div class="mb-1 flex items-center justify-between font-sans text-xs text-[var(--ui-text-muted)]">
                      <span>Request ID</span>
                      <button
                        v-if="record.requestId"
                        type="button"
                        class="text-[var(--ui-text-muted)] transition hover:text-[var(--ui-text)]"
                        title="复制完整 Request ID"
                        @click="emit('copy-text', record.requestId, `req-${record.requestId}`, 'Request ID')"
                      >
                        <Check v-if="copiedKey === `req-${record.requestId}`" :size="12" class="text-[var(--ui-success)]" />
                        <Copy v-else :size="12" />
                      </button>
                    </div>
                    <span class="select-all break-all text-[var(--ui-text)]">
                      {{ record.requestId || '—' }}
                    </span>
                  </div>

                  <!-- Session ID -->
                  <div class="rounded-lg border border-[var(--ui-line)] bg-[var(--ui-surface)] p-2.5">
                    <div class="mb-1 flex items-center justify-between font-sans text-xs text-[var(--ui-text-muted)]">
                      <span>Session ID</span>
                      <button
                        v-if="record.sessionId"
                        type="button"
                        class="text-[var(--ui-text-muted)] transition hover:text-[var(--ui-text)]"
                        title="复制 Session ID"
                        @click="emit('copy-text', record.sessionId, `sess-${record.sessionId}`, 'Session ID')"
                      >
                        <Check v-if="copiedKey === `sess-${record.sessionId}`" :size="12" class="text-[var(--ui-success)]" />
                        <Copy v-else :size="12" />
                      </button>
                    </div>
                    <span class="select-all break-all text-[var(--ui-text)]">
                      {{ record.sessionId || '—' }}
                    </span>
                  </div>

                  <!-- Client ID / Auth Method -->
                  <div class="rounded-lg border border-[var(--ui-line)] bg-[var(--ui-surface)] p-2.5">
                    <div class="mb-1 flex items-center justify-between font-sans text-xs text-[var(--ui-text-muted)]">
                      <span>认证与客户端</span>
                      <button
                        v-if="record.clientId"
                        type="button"
                        class="text-[var(--ui-text-muted)] transition hover:text-[var(--ui-text)]"
                        title="复制 Client ID"
                        @click="emit('copy-text', record.clientId, `cli-${record.clientId}`, 'Client ID')"
                      >
                        <Check v-if="copiedKey === `cli-${record.clientId}`" :size="12" class="text-[var(--ui-success)]" />
                        <Copy v-else :size="12" />
                      </button>
                    </div>
                    <span class="select-all break-all text-[var(--ui-text)]">
                      {{ record.clientId ? `${record.authMethod}: ${record.clientId}` : record.authMethod }}
                    </span>
                  </div>

                  <!-- Change ID -->
                  <div v-if="record.changeId" class="rounded-lg border border-[var(--ui-line)] bg-[var(--ui-surface)] p-2.5 sm:col-span-2">
                    <div class="mb-1 flex items-center justify-between font-sans text-xs text-[var(--ui-text-muted)]">
                      <span class="flex items-center gap-1 text-[var(--ui-accent)]">
                        <Zap :size="11" /> 状态变更摘要 (Change ID)
                      </span>
                      <button
                        type="button"
                        class="text-[var(--ui-text-muted)] transition hover:text-[var(--ui-text)]"
                        title="复制 Change ID"
                        @click="emit('copy-text', record.changeId!, `chg-${record.changeId}`, 'Change ID')"
                      >
                        <Check v-if="copiedKey === `chg-${record.changeId}`" :size="12" class="text-[var(--ui-success)]" />
                        <Copy v-else :size="12" />
                      </button>
                    </div>
                    <div class="flex items-baseline justify-between gap-2">
                      <code class="select-all break-all text-[var(--ui-accent)]">{{ record.changeId }}</code>
                      <span class="shrink-0 font-sans text-xs text-[var(--ui-text-muted)]">
                        涉及 {{ record.changedFileCount ?? 0 }} 个受影响文件
                      </span>
                    </div>
                  </div>

                  <!-- Execution Timestamp & Payload Details -->
                  <div class="rounded-lg border border-[var(--ui-line)] bg-[var(--ui-surface)] p-2.5 font-sans" :class="{ 'sm:col-span-2 lg:col-span-1': !record.changeId }">
                    <div class="mb-1 text-xs text-[var(--ui-text-muted)]">执行绝对时间与体积</div>
                    <div class="space-y-0.5 font-mono text-xs text-[var(--ui-text-secondary)]">
                      <p>时间戳: {{ formatters.formatTime(record.timestampMs) }}</p>
                      <p>请求体: {{ formatters.formatBytes(record.requestBytes) }} ({{ record.requestBytes }} B)</p>
                      <p>响应体: {{ formatters.formatBytes(record.responseBytes) }} ({{ record.responseBytes }} B)</p>
                    </div>
                  </div>
                </div>
              </div>
            </transition>
          </article>
</template>
