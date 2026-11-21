<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import AgentContextInspector from "./AgentContextInspector.vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import SaveBar from "../ui/SaveBar.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import TextAreaField from "$src/components/ui/TextAreaField.vue";
import TextField from "$src/components/ui/TextField.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import { AGENT_SOURCE_OPTIONS, toggleSource } from "$lib/agent-context";
import { scanAgentContext, type AgentContextSnapshotDto } from "$lib/api/agent-context";
import { showToast } from "$lib/stores/toast";
import { useUnsavedChanges } from "$lib/unsaved-changes";
import type { RuntimePolicyDraft } from "$lib/workspace-page";

const TOOL_OPTIONS = [
  { value: "compact", label: "精简开发（推荐）" },
  { value: "core", label: "兼容核心" },
  { value: "advanced", label: "完整工具" },
  { value: "read-only", label: "只读工具" },
  { value: "compat-readonly-all", label: "兼容只读" },
];

const PERMISSION_OPTIONS = [
  { value: "trusted", label: "受信任" },
  { value: "safe", label: "安全受限" },
  { value: "dangerous", label: "完全放开" },
];

function clonePolicyDraft(source: RuntimePolicyDraft): RuntimePolicyDraft {
  return {
    ...source,
    instructionSources: [...source.instructionSources],
    skillSources: [...source.skillSources],
  };
}

function sameStringArray(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((item, index) => item === b[index]);
}

function isRuntimePolicyEqual(a: RuntimePolicyDraft, b: RuntimePolicyDraft): boolean {
  return (
    a.toolProfile === b.toolProfile &&
    a.permissionMode === b.permissionMode &&
    a.inheritGlobalExecutionPolicy === b.inheritGlobalExecutionPolicy &&
    a.allowedCommands === b.allowedCommands &&
    a.executablePaths === b.executablePaths &&
    a.aiInstructions === b.aiInstructions &&
    a.customInstructionPaths === b.customInstructionPaths &&
    a.customSkillPaths === b.customSkillPaths &&
    a.workspaceLocalEntries === b.workspaceLocalEntries &&
    a.workspaceScriptExtensions === b.workspaceScriptExtensions &&
    sameStringArray(a.instructionSources, b.instructionSources) &&
    sameStringArray(a.skillSources, b.skillSources)
  );
}

const props = withDefaults(
  defineProps<{
    workspaceId: string;
    model: RuntimePolicyDraft;
    section?: "all" | "policy" | "context";
    onSave: (draft: RuntimePolicyDraft) => Promise<void>;
  }>(),
  { section: "all" },
);

const draft = reactive<RuntimePolicyDraft>(clonePolicyDraft(props.model));
const baseline = ref<RuntimePolicyDraft>(clonePolicyDraft(props.model));
const saving = ref(false);
const scanning = ref(false);
const scanResult = ref<AgentContextSnapshotDto | null>(null);
const scanError = ref("");
let persistPromise: Promise<boolean> | null = null;

function syncModel() {
  const next = clonePolicyDraft(props.model);
  Object.assign(draft, next);
  baseline.value = clonePolicyDraft(next);
}

const dirty = computed(() => !isRuntimePolicyEqual(draft, baseline.value));

watch(
  () => props.model,
  () => {
    if (!dirty.value) syncModel();
  },
);

function toggleInstruction(value: string, checked: boolean) {
  draft.instructionSources = toggleSource(draft.instructionSources, value, checked);
}

function toggleSkill(value: string, checked: boolean) {
  draft.skillSources = toggleSource(draft.skillSources, value, checked);
}

function normalizedDraft(): RuntimePolicyDraft {
  return {
    ...draft,
    allowedCommands: draft.allowedCommands.trim(),
    executablePaths: draft.executablePaths.trim(),
    aiInstructions: draft.aiInstructions.trim(),
    customInstructionPaths: draft.customInstructionPaths.trim(),
    customSkillPaths: draft.customSkillPaths.trim(),
    workspaceScriptExtensions: draft.workspaceScriptExtensions.trim(),
    instructionSources: [...draft.instructionSources],
    skillSources: [...draft.skillSources],
  };
}

async function persist() {
  if (persistPromise) return persistPromise;
  if (!dirty.value) return false;
  saving.value = true;
  const payload = normalizedDraft();
  persistPromise = (async () => {
    await props.onSave(payload);
    Object.assign(draft, clonePolicyDraft(payload));
    baseline.value = clonePolicyDraft(payload);
    scanResult.value = null;
    return true;
  })();
  try {
    return await persistPromise;
  } finally {
    persistPromise = null;
    saving.value = false;
  }
}

async function save() {
  scanError.value = "";
  try {
    if (await persist()) {
      showToast("工作区执行策略已保存，将从下一次工具调用起生效。", { kind: "success" });
    }
    return true;
  } catch (error) {
    scanError.value = String(error);
    return false;
  }
}

async function scan() {
  if (scanning.value) return;
  if (dirty.value) {
    scanError.value = "请先保存策略，再扫描实际生效的来源。";
    return;
  }
  scanning.value = true;
  scanError.value = "";
  try {
    scanResult.value = await scanAgentContext(props.workspaceId);
  } catch (error) {
    scanError.value = String(error);
  } finally {
    scanning.value = false;
  }
}

useUnsavedChanges("工作区执行策略", dirty, save);
</script>

<template>
  <fieldset :disabled="saving">
    <form class="grid gap-4" @submit.prevent="save">
      <!-- 权限、命令白名单与本地脚本入口 -->
      <template v-if="section === 'all' || section === 'policy'">
        <div class="ui-inset p-3.5">
          <ToggleSwitch
            v-model="draft.inheritGlobalExecutionPolicy"
            label="继承全局执行权限"
            description="使用「应用设置 → 默认权限与命令白名单」中的全局权限模式与白名单；关闭后可为当前工作区单独覆盖。"
          />
        </div>
        <div class="runtime-two-col">
          <SelectField v-model="draft.toolProfile" label="工具档位" :options="TOOL_OPTIONS" />
          <SelectField
            v-model="draft.permissionMode"
            label="权限模式"
            :options="PERMISSION_OPTIONS"
            :disabled="draft.inheritGlobalExecutionPolicy"
            :hint="draft.inheritGlobalExecutionPolicy ? '当前由全局执行权限控制。' : undefined"
          />
        </div>
        <TextField
          v-model="draft.allowedCommands"
          label="允许执行的系统命令（逗号分隔）"
          placeholder="python,git,gh,aws,cargo,..."
          :disabled="draft.inheritGlobalExecutionPolicy"
          :hint="draft.inheritGlobalExecutionPolicy ? '当前继承全局命令白名单。' : undefined"
        />
        <TextAreaField
          v-model="draft.executablePaths"
          label="额外可执行 PATH（每行一个目录）"
          :rows="4"
          mono
          placeholder="/opt/homebrew/bin&#10;/usr/local/bin&#10;~/.cargo/bin"
          hint="工作区 PATH 优先于全局 PATH，再回退系统 PATH；命令仍需在白名单内。"
        />
        <div class="ui-inset p-3.5">
          <ToggleSwitch
            v-model="draft.workspaceLocalEntries"
            label="允许执行工作区内本地脚本入口"
            description="允许通过项目相对路径直接执行仓库内部构建脚本与本地工具。"
          />
        </div>
        <TextField
          v-model="draft.workspaceScriptExtensions"
          label="本地脚本扩展名（逗号分隔）"
          placeholder=".exe,.bat,.cmd,.ps1"
          :disabled="!draft.workspaceLocalEntries"
        />
      </template>

      <!-- AI 指令与技能来源 -->
      <template v-if="section === 'all' || section === 'context'">
        <TextAreaField
          v-model="draft.aiInstructions"
          label="工作区专属 AI 指令"
          :rows="4"
          placeholder="例如：修改代码必须遵循 Clean Code；运行测试通过后再提交验收。"
        />

        <div class="ui-inset space-y-4 p-4">
          <div>
            <p class="text-sm font-semibold">AI 指令与技能来源</p>
            <p class="mt-0.5 text-xs leading-5 text-[var(--ui-text-muted)]">
              未勾选时自动继承全局设置；精简开发档位仅常驻核心规则，其余技能按需检索读取。
            </p>
          </div>

          <div class="runtime-two-col">
            <div>
              <p class="mb-2 text-xs font-semibold tracking-wider text-[var(--ui-text-muted)]">指令来源（Instructions）</p>
              <label
                v-for="option in AGENT_SOURCE_OPTIONS"
                :key="option.value"
                class="mb-2 flex cursor-pointer gap-2.5 rounded-2xl border border-[var(--ui-line)] bg-[var(--ui-surface)] p-2.5"
              >
                <input
                  type="checkbox"
                  :checked="draft.instructionSources.includes(option.value)"
                  @change="toggleInstruction(option.value, ($event.target as HTMLInputElement).checked)"
                />
                <span>
                  <strong class="block text-xs">{{ option.label }}</strong>
                  <small class="text-xs text-[var(--ui-text-muted)]">{{ option.detail }}</small>
                </span>
              </label>
            </div>
            <div>
              <p class="mb-2 text-xs font-semibold tracking-wider text-[var(--ui-text-muted)]">技能来源（Skills）</p>
              <label
                v-for="option in AGENT_SOURCE_OPTIONS"
                :key="option.value"
                class="mb-2 flex cursor-pointer gap-2.5 rounded-2xl border border-[var(--ui-line)] bg-[var(--ui-surface)] p-2.5"
              >
                <input
                  type="checkbox"
                  :checked="draft.skillSources.includes(option.value)"
                  @change="toggleSkill(option.value, ($event.target as HTMLInputElement).checked)"
                />
                <span>
                  <strong class="block text-xs">{{ option.label }}</strong>
                  <small class="text-xs text-[var(--ui-text-muted)]">{{ option.detail }}</small>
                </span>
              </label>
            </div>
          </div>

          <div
            v-if="draft.instructionSources.includes('custom') || draft.skillSources.includes('custom')"
            class="runtime-two-col border-t border-[var(--ui-line)] pt-3"
          >
            <TextAreaField
              v-if="draft.instructionSources.includes('custom')"
              v-model="draft.customInstructionPaths"
              label="自定义 Instructions 文件"
              mono
              placeholder="docs/AI_RULES.md"
            />
            <TextAreaField
              v-if="draft.skillSources.includes('custom')"
              v-model="draft.customSkillPaths"
              label="自定义 Skills 根目录"
              mono
              placeholder=".my-agent/skills"
            />
          </div>

          <div class="flex flex-wrap items-center gap-3 border-t border-[var(--ui-line)] pt-3">
            <BaseButton variant="secondary" size="sm" :busy="scanning" @click="scan">扫描实际生效来源</BaseButton>
            <span v-if="scanResult" class="text-xs text-[var(--ui-text-muted)]">
              已发现 {{ scanResult.instructions.length }} 个 Instructions · {{ scanResult.skills.length }} 个 Skills
            </span>
            <span v-if="scanError" class="text-xs text-[var(--ui-danger)]">{{ scanError }}</span>
          </div>

          <AgentContextInspector
            v-if="scanResult && (scanResult.instructions.length || scanResult.skills.length)"
            :scan-result="scanResult"
          />
        </div>
      </template>

      <SaveBar :dirty="dirty" :busy="saving" :error="scanError" label="保存策略" @save="save" />
    </form>
  </fieldset>
</template>

<style scoped>
.runtime-two-col {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 0.75rem;
  min-width: 0;
}

@container workspace-page (max-width: 700px) {
  .runtime-two-col {
    grid-template-columns: 1fr;
  }
}
</style>
