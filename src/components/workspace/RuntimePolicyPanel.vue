<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import TextAreaField from "$src/components/ui/TextAreaField.vue";
import TextField from "$src/components/ui/TextField.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import { scanAgentContext, type AgentContextSnapshotDto } from "$lib/api/agent-context";
import { AGENT_SOURCE_OPTIONS, toggleSource } from "$lib/agent-context";
import type { RuntimePolicyDraft } from "$lib/workspace-page";

const props = defineProps<{
  workspaceId: string;
  model: RuntimePolicyDraft;
}>();

const emit = defineEmits<{ save: [draft: RuntimePolicyDraft] }>();

const toolOptions = [
  { value: "compact", label: "精简开发（推荐）" },
  { value: "core", label: "兼容核心" },
  { value: "advanced", label: "完整工具" },
  { value: "read-only", label: "只读工具" },
  { value: "compat-readonly-all", label: "兼容只读" },
];
const permissionOptions = [
  { value: "trusted", label: "受信任" },
  { value: "safe", label: "安全受限" },
  { value: "dangerous", label: "完全放开" },
];

const draft = reactive<RuntimePolicyDraft>({ ...props.model, instructionSources: [], skillSources: [] });
const baseline = ref("");
const saving = ref(false);
const scanning = ref(false);
const scanResult = ref<AgentContextSnapshotDto | null>(null);
const scanError = ref("");

function syncModel() {
  Object.assign(draft, props.model, {
    instructionSources: [...props.model.instructionSources],
    skillSources: [...props.model.skillSources],
  });
  baseline.value = JSON.stringify(draft);
}

watch(() => props.model, syncModel, { immediate: true, deep: true });
const dirty = computed(() => JSON.stringify(draft) !== baseline.value);

function toggleInstruction(value: string, checked: boolean) {
  draft.instructionSources = toggleSource(draft.instructionSources, value, checked);
}
function toggleSkill(value: string, checked: boolean) {
  draft.skillSources = toggleSource(draft.skillSources, value, checked);
}

async function save() {
  if (saving.value || !dirty.value) return;
  saving.value = true;
  try {
    emit("save", {
      ...draft,
      allowedCommands: draft.allowedCommands.trim(),
      executablePaths: draft.executablePaths.trim(),
      aiInstructions: draft.aiInstructions.trim(),
      customInstructionPaths: draft.customInstructionPaths.trim(),
      customSkillPaths: draft.customSkillPaths.trim(),
      workspaceScriptExtensions: draft.workspaceScriptExtensions.trim(),
      instructionSources: [...draft.instructionSources],
      skillSources: [...draft.skillSources],
    });
    baseline.value = JSON.stringify(draft);
    scanResult.value = null;
  } finally {
    saving.value = false;
  }
}

async function scan() {
  if (scanning.value) return;
  scanning.value = true;
  scanError.value = "";
  try {
    if (dirty.value) await save();
    scanResult.value = await scanAgentContext(props.workspaceId);
  } catch (error) {
    scanError.value = String(error);
  } finally {
    scanning.value = false;
  }
}
</script>

<template>
  <form class="grid gap-4" @submit.prevent="save">
    <div class="grid gap-3 md:grid-cols-2">
      <SelectField v-model="draft.toolProfile" label="工具档位" :options="toolOptions" />
      <SelectField v-model="draft.permissionMode" label="权限模式" :options="permissionOptions" />
    </div>
    <TextField v-model="draft.allowedCommands" label="系统命令（逗号分隔）" placeholder="python,git,gh,aws,cargo,..." />
    <TextAreaField v-model="draft.executablePaths" label="额外可执行 PATH" mono placeholder="/opt/homebrew/bin\n/usr/local/bin\n~/.cargo/bin" hint="Workspace PATH 优先于 Global PATH，再回退 System PATH；命令仍需加入白名单。" />

    <div class="ios-glass ios-card-surface p-4">
      <p class="text-sm font-semibold">Agent Context Sources</p>
      <p class="mt-1 text-xs leading-5 text-[var(--text-muted)]">Workspace 未选择来源时继承全局设置；compact 档位只常驻核心规则，其余按需读取。</p>
      <div class="mt-4 grid gap-4 md:grid-cols-2">
        <div>
          <p class="mb-2 text-[10px] font-semibold uppercase tracking-[.1em] text-[var(--text-muted)]">Instructions</p>
          <label v-for="option in AGENT_SOURCE_OPTIONS" :key="option.value" class="mb-2 flex cursor-pointer gap-2.5 rounded-2xl bg-white/35 p-2.5 dark:bg-white/4">
            <input type="checkbox" :checked="draft.instructionSources.includes(option.value)" @change="toggleInstruction(option.value, ($event.target as HTMLInputElement).checked)" />
            <span><strong class="block text-xs">{{ option.label }}</strong><small class="text-[10px] text-[var(--text-muted)]">{{ option.detail }}</small></span>
          </label>
        </div>
        <div>
          <p class="mb-2 text-[10px] font-semibold uppercase tracking-[.1em] text-[var(--text-muted)]">Skills</p>
          <label v-for="option in AGENT_SOURCE_OPTIONS" :key="option.value" class="mb-2 flex cursor-pointer gap-2.5 rounded-2xl bg-white/35 p-2.5 dark:bg-white/4">
            <input type="checkbox" :checked="draft.skillSources.includes(option.value)" @change="toggleSkill(option.value, ($event.target as HTMLInputElement).checked)" />
            <span><strong class="block text-xs">{{ option.label }}</strong><small class="text-[10px] text-[var(--text-muted)]">{{ option.detail }}</small></span>
          </label>
        </div>
      </div>
      <div v-if="draft.instructionSources.includes('custom') || draft.skillSources.includes('custom')" class="mt-3 grid gap-3 border-t border-white/40 pt-3 md:grid-cols-2 dark:border-white/8">
        <TextAreaField v-if="draft.instructionSources.includes('custom')" v-model="draft.customInstructionPaths" label="自定义 Instructions 文件" mono placeholder="docs/AI_RULES.md" />
        <TextAreaField v-if="draft.skillSources.includes('custom')" v-model="draft.customSkillPaths" label="自定义 Skills 根目录" mono placeholder=".my-agent/skills" />
      </div>
      <div class="mt-3 flex flex-wrap items-center gap-3 border-t border-white/40 pt-3 dark:border-white/8">
        <BaseButton variant="secondary" size="sm" :busy="scanning" @click="scan">立即重新扫描</BaseButton>
        <span v-if="scanResult" class="text-xs text-[var(--text-muted)]">已发现 {{ scanResult.instructions.length }} 个 Instructions · {{ scanResult.skills.length }} 个 Skills</span>
        <span v-if="scanError" class="text-xs text-[var(--danger)]">{{ scanError }}</span>
      </div>
      <details v-if="scanResult && (scanResult.instructions.length || scanResult.skills.length)" class="mt-3 text-xs">
        <summary class="cursor-pointer text-[var(--text-muted)]">查看发现结果</summary>
        <div class="mt-2 rounded-2xl bg-black/4 p-3 font-mono text-[10px] dark:bg-white/4">
          <div v-for="item in scanResult.instructions" :key="item.path">[{{ item.provider }}] {{ item.path }}</div>
          <div v-for="skill in scanResult.skills" :key="skill.path">Skill · {{ skill.name }} · {{ skill.provider }} · {{ skill.path }}</div>
        </div>
      </details>
    </div>

    <TextAreaField v-model="draft.aiInstructions" label="Workspace AI Instructions" :rows="5" placeholder="例如：修改代码必须遵循 Clean Code；运行测试后再确认完成。" />
    <div class="ios-glass ios-inset-surface p-3"><ToggleSwitch v-model="draft.workspaceLocalEntries" label="允许执行 Workspace 内本地入口" description="允许通过本地相对路径直接执行工作区内部工具与脚本" /></div>
    <TextField v-model="draft.workspaceScriptExtensions" label="本地脚本扩展名（逗号分隔）" placeholder=".exe,.bat,.cmd,.ps1" :disabled="!draft.workspaceLocalEntries" />
    <div class="flex justify-end"><BaseButton variant="primary" :busy="saving" :disabled="!dirty" @click="save">保存策略</BaseButton></div>
  </form>
</template>
