<script setup lang="ts">
import { computed, ref } from "vue";
import { AlertTriangle, ArrowRightLeft, ChevronDown, ChevronRight, FileJson, Lock, Radar } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import ModalDialog from "$src/components/ui/ModalDialog.vue";
import SegmentedControl from "$src/components/ui/SegmentedControl.vue";
import TextAreaField from "$src/components/ui/TextAreaField.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import type {
  HarnessTakeoverPlanItem,
  ImportedUpstreamCandidate,
  TakeoverConnectMode,
  UpstreamExposeMode,
} from "$lib/api/upstream-mcp";
import {
  transportLabel,
  type ImportCandidateSelectionDraft,
} from "$lib/upstream-mcp/use-upstream-mcp";

const props = defineProps<{
  open: boolean;
  tab: "scan" | "json";
  scanning: boolean;
  parsing: boolean;
  importing: boolean;
  error: string;
  jsonText: string;
  candidates: ImportedUpstreamCandidate[];
  selections: Record<string, ImportCandidateSelectionDraft>;
  replaceSourceConfigs: boolean;
  connectMode: TakeoverConnectMode;
  takeoverPreview: HarnessTakeoverPlanItem[];
  loadingTakeoverPreview: boolean;
}>();

const emit = defineEmits<{
  close: [];
  "update:tab": [tab: "scan" | "json"];
  "update:jsonText": [value: string];
  "update:replaceSourceConfigs": [value: boolean];
  "update:connectMode": [value: TakeoverConnectMode];
  "update-selection": [
    payload: {
      id: string;
      field: keyof ImportCandidateSelectionDraft;
      value: ImportCandidateSelectionDraft[keyof ImportCandidateSelectionDraft];
    },
  ];
  rescan: [];
  "parse-json": [];
  "submit-import": [];
}>();

const TAB_ITEMS = [
  { value: "scan", label: "扫描本机客户端" },
  { value: "json", label: "粘贴 mcpServers JSON" },
];

const CONNECT_MODE_ITEMS = [
  { value: "auto", label: "自动最佳" },
  { value: "http", label: "本机 HTTP" },
  { value: "stdio", label: "mcp-stdio 桥接" },
];

const expandedPreviewKey = ref<string | null>(null);

const selectedCount = computed(
  () =>
    props.candidates.filter(
      (c) => !c.blockedReason && props.selections[c.candidateId]?.selected,
    ).length,
);

const hasBlockingPreviewWarning = computed(
  () =>
    props.tab === "scan" &&
    props.replaceSourceConfigs &&
    props.takeoverPreview.some((p) => p.effectiveMode === "http" && Boolean(p.warning)),
);

function setSelectionField<K extends keyof ImportCandidateSelectionDraft>(
  id: string,
  field: K,
  value: ImportCandidateSelectionDraft[K],
) {
  emit("update-selection", { id, field, value });
}
</script>

<template>
  <ModalDialog
    :open="open"
    title="导入上游 MCP 配置"
    description="支持只读扫描 Claude / Cursor / Antigravity / Gemini / Codex / OpenCode 本地配置，或直接粘贴标准 mcpServers JSON；敏感 Token 会自动归入加密存储。"
    :dismissible="!importing"
    @close="emit('close')"
  >
    <div class="max-h-[72vh] space-y-4 overflow-y-auto pr-1">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <SegmentedControl
          :items="TAB_ITEMS"
          :model-value="tab"
          aria-label="选择导入方式"
          @update:model-value="emit('update:tab', $event as 'scan' | 'json')"
        />
        <BaseButton
          v-if="tab === 'scan'"
          variant="secondary"
          size="sm"
          :busy="scanning"
          @click="emit('rescan')"
        >
          <Radar v-if="!scanning" :size="13" />
          重新扫描
        </BaseButton>
      </div>

      <div v-if="tab === 'json'" class="space-y-2">
        <TextAreaField
          :model-value="jsonText"
          label="粘贴 mcpServers / servers JSON"
          placeholder='{&#10;  "mcpServers": {&#10;    "github": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"] }&#10;  }&#10;}'
          :rows="6"
          @update:model-value="emit('update:jsonText', $event)"
        />
        <div class="flex justify-end">
          <BaseButton variant="secondary" size="sm" :busy="parsing" @click="emit('parse-json')">
            <FileJson v-if="!parsing" :size="13" />
            解析并预览
          </BaseButton>
        </div>
      </div>

      <div
        v-if="error"
        role="alert"
        class="rounded-[var(--ui-radius-control)] border border-[var(--ui-danger)]/30 bg-[var(--ui-danger-soft)] px-3 py-2 text-xs text-[var(--ui-danger)]"
      >
        {{ error }}
      </div>

      <p v-if="tab === 'scan' && scanning" class="py-6 text-center text-xs text-[var(--ui-text-muted)]">
        正在只读扫描本机 AI 客户端 MCP 配置…
      </p>

      <p
        v-else-if="!candidates.length"
        class="rounded-[var(--ui-radius-control)] border border-dashed border-[var(--ui-line)] py-6 text-center text-xs text-[var(--ui-text-muted)]"
      >
        {{ tab === 'scan' ? '未在本机常见客户端目录发现可导入的 MCP 配置，可切换到“粘贴 JSON”手动导入。' : '粘贴 JSON 后点击“解析并预览”查看待导入条目。' }}
      </p>

      <div v-else class="space-y-2.5">
        <div
          v-for="item in candidates"
          :key="item.candidateId"
          class="rounded-[var(--ui-radius-control)] border p-3 transition"
          :class="
            item.blockedReason
              ? 'border-[var(--ui-danger)]/25 bg-[var(--ui-surface-subtle)] opacity-65'
              : selections[item.candidateId]?.selected
                ? 'border-[var(--ui-accent)]/50 bg-[var(--ui-accent-soft)]/30'
                : 'border-[var(--ui-line)] bg-[var(--ui-surface-subtle)]'
          "
        >
          <div class="flex items-start gap-2.5">
            <input
              type="checkbox"
              class="mt-1 h-4 w-4 accent-[var(--ui-accent)]"
              :disabled="Boolean(item.blockedReason)"
              :checked="Boolean(!item.blockedReason && selections[item.candidateId]?.selected)"
              @change="setSelectionField(item.candidateId, 'selected', ($event.target as HTMLInputElement).checked)"
            />
            <div class="min-w-0 flex-1 space-y-1.5">
              <div class="flex flex-wrap items-center gap-1.5">
                <span class="text-sm font-medium text-[var(--ui-text)]">{{ item.name }}</span>
                <span class="rounded-full border border-[var(--ui-line)] px-1.5 py-0.5 text-[10px] text-[var(--ui-text-secondary)]">
                  {{ item.sourceLabel }}
                </span>
                <span class="rounded-full bg-[var(--ui-surface-raised)] px-1.5 py-0.5 text-[10px] text-[var(--ui-text-muted)]">
                  {{ transportLabel(item.transportKind) }}
                </span>
                <span
                  v-if="item.secretEnvKeys.length + item.secretHeaderKeys.length + (item.hasBearerToken ? 1 : 0) > 0"
                  class="inline-flex items-center gap-0.5 rounded-full bg-[var(--ui-accent-soft)] px-1.5 py-0.5 text-[10px] text-[var(--ui-accent)]"
                >
                  <Lock :size="10" />
                  自动加密 {{ item.secretEnvKeys.length + item.secretHeaderKeys.length + (item.hasBearerToken ? 1 : 0) }} 项密钥
                </span>
              </div>

              <p class="truncate font-mono text-[11px] text-[var(--ui-text-muted)]" :title="item.summaryTarget">
                {{ item.summaryTarget }}
              </p>
              <p v-if="item.sourcePath" class="font-mono text-[10px] text-[var(--ui-text-muted)]">
                来源：{{ item.sourcePath }}
              </p>

              <div
                v-if="item.blockedReason"
                class="flex items-center gap-1.5 text-xs text-[var(--ui-danger)]"
              >
                <AlertTriangle :size="12" class="shrink-0" />
                <span>{{ item.blockedReason }}</span>
              </div>

              <div
                v-else-if="selections[item.candidateId]?.selected"
                class="flex flex-wrap items-center gap-2 pt-1"
              >
                <label class="flex items-center gap-1.5 text-xs text-[var(--ui-text-secondary)]">
                  <span>别名</span>
                  <input
                    type="text"
                    :value="selections[item.candidateId]?.alias ?? item.suggestedAlias"
                    class="h-7 w-32 rounded-md border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] px-2 font-mono text-xs text-[var(--ui-text)] outline-none focus:border-[var(--ui-accent)]"
                    @input="setSelectionField(item.candidateId, 'alias', ($event.target as HTMLInputElement).value)"
                  />
                </label>
                <select
                  :value="selections[item.candidateId]?.exposeMode ?? 'direct'"
                  class="h-7 rounded-md border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] px-2 text-xs text-[var(--ui-text)] outline-none"
                  @change="setSelectionField(item.candidateId, 'exposeMode', ($event.target as HTMLSelectElement).value as UpstreamExposeMode)"
                >
                  <option value="direct">直出（前缀）</option>
                  <option value="proxy">懒加载代理</option>
                </select>
                <span v-if="item.alreadyExistsAlias" class="text-[11px] text-[var(--ui-warning)]">
                  原别名已存在，已自动建议新别名
                </span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <!-- Aggregate & Replace (Harness Takeover) Option -->
      <div
        v-if="tab === 'scan' && candidates.length"
        class="space-y-3 rounded-[var(--ui-radius-control)] border p-3.5 transition"
        :class="
          replaceSourceConfigs
            ? 'border-[var(--ui-accent)]/45 bg-[var(--ui-accent-soft)]/25'
            : 'border-[var(--ui-line)] bg-[var(--ui-surface-subtle)]'
        "
      >
        <ToggleSwitch
          :model-value="replaceSourceConfigs"
          :disabled="importing"
          label="聚合并替换原客户端配置（自动备份，可一键还原）"
          description="从所选客户端移除上述被接管的旧 MCP（保留未勾选 MCP 与其他所有配置/注释），并写入统一的 Coding Tools MCP 入口"
          @update:model-value="emit('update:replaceSourceConfigs', $event)"
        />

        <template v-if="replaceSourceConfigs">
          <div class="flex flex-wrap items-center justify-between gap-2 border-t border-[var(--ui-line)] pt-2.5">
            <span class="flex items-center gap-1.5 text-xs font-medium text-[var(--ui-text-secondary)]">
              <ArrowRightLeft :size="13" class="text-[var(--ui-accent)]" />
              替换写入的接入方式
            </span>
            <SegmentedControl
              :items="CONNECT_MODE_ITEMS"
              :model-value="connectMode"
              :disabled="importing"
              aria-label="选择替换写入的接入方式"
              @update:model-value="emit('update:connectMode', $event as TakeoverConnectMode)"
            />
          </div>

          <p v-if="loadingTakeoverPreview" class="text-xs text-[var(--ui-text-muted)]">
            正在生成原配置文件变更预览…
          </p>

          <div v-else-if="takeoverPreview.length" class="space-y-2">
            <div
              v-for="plan in takeoverPreview"
              :key="plan.sourceClient"
              class="rounded-md border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] p-2.5 text-xs"
            >
              <div class="flex items-center justify-between gap-2">
                <div class="flex flex-wrap items-center gap-1.5">
                  <span class="font-semibold text-[var(--ui-text)]">{{ plan.sourceLabel }}</span>
                  <span class="font-mono text-[11px] text-[var(--ui-text-muted)]">{{ plan.displayPath }}</span>
                  <span class="rounded-full bg-[var(--ui-accent-soft)] px-1.5 py-0.5 text-[10px] font-medium text-[var(--ui-accent)]">
                    {{ plan.effectiveMode === 'http' ? '本机 HTTP 直连' : 'mcp-stdio 桥接' }}
                  </span>
                </div>
                <button
                  type="button"
                  class="flex items-center gap-0.5 text-[11px] text-[var(--ui-text-muted)] hover:text-[var(--ui-text)]"
                  @click="expandedPreviewKey = expandedPreviewKey === plan.sourceClient ? null : plan.sourceClient"
                >
                  <span>配置片段</span>
                  <ChevronDown v-if="expandedPreviewKey === plan.sourceClient" :size="12" />
                  <ChevronRight v-else :size="12" />
                </button>
              </div>

              <div class="mt-1.5 space-y-0.5 font-mono text-[11px]">
                <div class="text-[var(--ui-danger)]">
                  - 移除：{{ plan.removedKeys.join(', ') }}
                </div>
                <div class="text-[var(--ui-success)]">
                  + {{ plan.alreadyHasCodingTools ? `保留已有入口：${plan.injectedKey}` : `写入统一入口：${plan.injectedKey}` }}
                </div>
              </div>

              <div
                v-if="plan.warning"
                class="mt-1.5 flex items-start gap-1.5 rounded bg-[var(--ui-warning-soft)] px-2 py-1 text-[11px] text-[var(--ui-warning)]"
              >
                <AlertTriangle :size="12" class="mt-0.5 shrink-0" />
                <span>{{ plan.warning }}</span>
              </div>

              <pre
                v-if="expandedPreviewKey === plan.sourceClient"
                class="mt-2 max-h-36 overflow-auto rounded bg-[var(--ui-surface-subtle)] p-2 font-mono text-[11px] text-[var(--ui-text-secondary)]"
              >{{ plan.entryPreview }}</pre>
            </div>
          </div>
        </template>
      </div>

      <div class="flex items-center justify-between gap-2 border-t border-[var(--ui-line)] pt-4">
        <span class="text-xs text-[var(--ui-text-muted)]">已选 {{ selectedCount }} 项</span>
        <div class="flex items-center gap-2">
          <BaseButton variant="secondary" :disabled="importing" @click="emit('close')">取消</BaseButton>
          <BaseButton
            variant="primary"
            :disabled="selectedCount === 0 || hasBlockingPreviewWarning"
            :busy="importing"
            @click="emit('submit-import')"
          >
            {{ tab === 'scan' && replaceSourceConfigs ? `聚合并替换 (${selectedCount})` : `确认导入 (${selectedCount})` }}
          </BaseButton>
        </div>
      </div>
    </div>
  </ModalDialog>
</template>
