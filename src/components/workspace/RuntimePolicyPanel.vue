<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue";
import { Check, Copy, FileCode, Layers, RefreshCw, Search, Sparkles } from "@lucide/vue";
import {useUnsavedChanges} from "$lib/unsaved-changes";
import SaveBar from "../ui/SaveBar.vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import TextAreaField from "$src/components/ui/TextAreaField.vue";
import TextField from "$src/components/ui/TextField.vue";
import ToggleSwitch from "$src/components/ui/ToggleSwitch.vue";
import { scanAgentContext, type AgentContextSnapshotDto } from "$lib/api/agent-context";
import { AGENT_SOURCE_OPTIONS, toggleSource } from "$lib/agent-context";
import { showToast } from "$lib/stores/toast";
import type { RuntimePolicyDraft } from "$lib/workspace-page";

const props = defineProps<{
  workspaceId: string;
  model: RuntimePolicyDraft;
  onSave: (draft: RuntimePolicyDraft) => Promise<void>;
}>();

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
const scanTab = ref<'all' | 'instructions' | 'skills'>('all');
const scanSearch = ref('');
const copiedPath = ref<string | null>(null);
let persistPromise: Promise<boolean> | null = null;

interface UnifiedScanItem {
  type: 'instruction' | 'skill';
  name: string;
  provider: string;
  path: string;
  description?: string;
  scope?: string;
}

const allScanItems = computed<UnifiedScanItem[]>(() => {
  if (!scanResult.value) return [];
  const instructions: UnifiedScanItem[] = (scanResult.value.instructions ?? []).map(item => ({
    type: 'instruction',
    name: item.path.split('/').pop() || item.path,
    provider: item.provider,
    path: item.path,
    scope: item.scope,
  }));
  const skills: UnifiedScanItem[] = (scanResult.value.skills ?? []).map(skill => ({
    type: 'skill',
    name: skill.name,
    provider: skill.provider,
    path: skill.path,
    description: skill.description,
    scope: skill.scope,
  }));
  return [...instructions, ...skills];
});

const filteredScanItems = computed(() => {
  return allScanItems.value.filter(item => {
    if (scanTab.value === 'instructions' && item.type !== 'instruction') return false;
    if (scanTab.value === 'skills' && item.type !== 'skill') return false;
    if (!scanSearch.value.trim()) return true;
    const q = scanSearch.value.trim().toLowerCase();
    return item.name.toLowerCase().includes(q) ||
           item.provider.toLowerCase().includes(q) ||
           item.path.toLowerCase().includes(q) ||
           (item.description || '').toLowerCase().includes(q);
  });
});

async function copyItemPath(path: string) {
  try {
    await navigator.clipboard.writeText(path);
    copiedPath.value = path;
    setTimeout(() => {
      if (copiedPath.value === path) copiedPath.value = null;
    }, 1500);
  } catch {
    showToast("复制路径失败", { kind: "error" });
  }
}

function providerBadgeClass(provider: string) {
  const p = (provider || '').toLowerCase();
  if (p === 'codex') return 'bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20';
  if (p === 'cursor') return 'bg-purple-500/10 text-purple-600 dark:text-purple-400 border border-purple-500/20';
  if (p === 'claude') return 'bg-amber-500/10 text-amber-600 dark:text-amber-400 border border-amber-500/20';
  if (p === 'opencode') return 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border border-emerald-500/20';
  if (p === 'reasonix') return 'bg-cyan-500/10 text-cyan-600 dark:text-cyan-400 border border-cyan-500/20';
  return 'bg-[var(--ui-surface-subtle)] text-[var(--ui-text-secondary)] border border-[var(--ui-line)]';
}

function syncModel() {
  Object.assign(draft, props.model, {
    instructionSources: [...props.model.instructionSources],
    skillSources: [...props.model.skillSources],
  });
  baseline.value = JSON.stringify(draft);
}

syncModel();
const dirty = computed(() => JSON.stringify(draft) !== baseline.value);
watch(() => props.model, () => { if(!dirty.value) syncModel(); }, {deep:true});

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
    Object.assign(draft, payload, {
      instructionSources: [...payload.instructionSources],
      skillSources: [...payload.skillSources],
    });
    baseline.value = JSON.stringify(draft);
    scanResult.value = null;
    return true;
  })();
  try {
    await persistPromise;
  } finally {
    persistPromise = null;
    saving.value = false;
  }
}

async function save() {
  scanError.value="";
  try {
    if (await persist()) {
      showToast("工作区执行策略已保存，将从下一次工具调用起生效。", { kind: "success" });
    }
    return true;
  } catch (error) {
    scanError.value=String(error);
    return false;
  }
}

async function scan() {
  if (scanning.value) return;
  if(dirty.value){scanError.value="请先保存策略，再扫描实际生效的来源。";return;}
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
useUnsavedChanges("工作区执行策略",dirty,save);
</script>

<template>
  <fieldset :disabled="saving"><form class="grid gap-4" @submit.prevent="save">
    <div class="ui-inset p-3">
      <ToggleSwitch
        v-model="draft.inheritGlobalExecutionPolicy"
        label="继承全局执行权限"
        description="使用 应用设置 → AI 执行 中的全局权限模式和命令白名单；关闭后可为当前 Workspace 单独覆盖。"
      />
    </div>
    <div class="runtime-two-col">
      <SelectField v-model="draft.toolProfile" label="工具档位" :options="toolOptions" />
      <SelectField v-model="draft.permissionMode" label="权限模式" :options="permissionOptions" :disabled="draft.inheritGlobalExecutionPolicy" :hint="draft.inheritGlobalExecutionPolicy ? '当前由全局执行权限控制。' : undefined" />
    </div>
    <TextField v-model="draft.allowedCommands" label="系统命令（逗号分隔）" placeholder="python,git,gh,aws,cargo,..." :disabled="draft.inheritGlobalExecutionPolicy" :hint="draft.inheritGlobalExecutionPolicy ? '当前继承全局命令白名单。' : undefined" />
    <TextAreaField v-model="draft.executablePaths" label="额外可执行 PATH" mono placeholder="/opt/homebrew/bin\n/usr/local/bin\n~/.cargo/bin" hint="Workspace PATH 优先于 Global PATH，再回退 System PATH；命令仍需加入白名单。" />

    <details class="ui-disclosure"><summary>AI 指令与技能来源</summary><div class="ui-disclosure-body">
      <p class="text-sm font-semibold">AI 指令与技能来源</p>
      <p class="mt-1 text-xs leading-5 text-[var(--text-muted)]">Workspace 未选择来源时继承全局设置；compact 档位只常驻核心规则，其余按需读取。</p>
      <div class="runtime-two-col mt-4">
        <div>
          <p class="mb-2 text-xs font-semibold uppercase tracking-[.1em] text-[var(--text-muted)]">指令来源</p>
          <label v-for="option in AGENT_SOURCE_OPTIONS" :key="option.value" class="mb-2 flex cursor-pointer gap-2.5 rounded-2xl bg-[var(--ui-surface-subtle)] p-2.5">
            <input type="checkbox" :checked="draft.instructionSources.includes(option.value)" @change="toggleInstruction(option.value, ($event.target as HTMLInputElement).checked)" />
            <span><strong class="block text-xs">{{ option.label }}</strong><small class="text-xs text-[var(--text-muted)]">{{ option.detail }}</small></span>
          </label>
        </div>
        <div>
          <p class="mb-2 text-xs font-semibold uppercase tracking-[.1em] text-[var(--text-muted)]">技能来源</p>
          <label v-for="option in AGENT_SOURCE_OPTIONS" :key="option.value" class="mb-2 flex cursor-pointer gap-2.5 rounded-2xl bg-[var(--ui-surface-subtle)] p-2.5">
            <input type="checkbox" :checked="draft.skillSources.includes(option.value)" @change="toggleSkill(option.value, ($event.target as HTMLInputElement).checked)" />
            <span><strong class="block text-xs">{{ option.label }}</strong><small class="text-xs text-[var(--text-muted)]">{{ option.detail }}</small></span>
          </label>
        </div>
      </div>
      <div v-if="draft.instructionSources.includes('custom') || draft.skillSources.includes('custom')" class="runtime-two-col mt-3 border-t border-[var(--ui-line)] pt-3 dark:border-[var(--ui-line)]">
        <TextAreaField v-if="draft.instructionSources.includes('custom')" v-model="draft.customInstructionPaths" label="自定义 Instructions 文件" mono placeholder="docs/AI_RULES.md" />
        <TextAreaField v-if="draft.skillSources.includes('custom')" v-model="draft.customSkillPaths" label="自定义 Skills 根目录" mono placeholder=".my-agent/skills" />
      </div>
      <div class="mt-3 flex flex-wrap items-center gap-3 border-t border-[var(--ui-line)] pt-3 dark:border-[var(--ui-line)]">
        <BaseButton variant="secondary" size="sm" :busy="scanning" @click="scan">立即重新扫描</BaseButton>
        <span v-if="scanResult" class="text-xs text-[var(--text-muted)]">已发现 {{ scanResult.instructions.length }} 个 Instructions · {{ scanResult.skills.length }} 个 Skills</span>
        <span v-if="scanError" class="text-xs text-[var(--danger)]">{{ scanError }}</span>
      </div>
      <!-- Structured Agent Context Inspector -->
      <details v-if="scanResult && (scanResult.instructions.length || scanResult.skills.length)" class="mt-3 text-xs" open>
        <summary class="cursor-pointer font-medium text-[var(--ui-text-secondary)] hover:text-[var(--ui-text)]">
          查看发现结果 ({{ allScanItems.length }})
        </summary>

        <div class="mt-2.5 rounded-2xl border border-[var(--ui-line)] bg-[var(--ui-surface-raised)] p-3 shadow-sm">
          <!-- Toolbar: Category switcher + search bar -->
          <div class="flex flex-wrap items-center justify-between gap-2.5 border-b border-[var(--ui-line)] pb-3">
            <div class="flex items-center gap-1">
              <button
                type="button"
                :class="scanTab === 'all' ? 'bg-[var(--ui-accent)] text-white shadow-xs' : 'text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)]'"
                class="rounded-lg px-2.5 py-1 text-xs font-medium transition cursor-pointer"
                @click="scanTab = 'all'"
              >
                全部 ({{ allScanItems.length }})
              </button>
              <button
                type="button"
                :class="scanTab === 'instructions' ? 'bg-[var(--ui-accent)] text-white shadow-xs' : 'text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)]'"
                class="rounded-lg px-2.5 py-1 text-xs font-medium transition cursor-pointer"
                @click="scanTab = 'instructions'"
              >
                指令 ({{ scanResult.instructions.length }})
              </button>
              <button
                type="button"
                :class="scanTab === 'skills' ? 'bg-[var(--ui-accent)] text-white shadow-xs' : 'text-[var(--ui-text-secondary)] hover:bg-[var(--ui-surface-hover)]'"
                class="rounded-lg px-2.5 py-1 text-xs font-medium transition cursor-pointer"
                @click="scanTab = 'skills'"
              >
                技能 ({{ scanResult.skills.length }})
              </button>
            </div>

            <!-- Search -->
            <div class="relative min-w-[180px] flex-1 max-w-xs">
              <Search :size="12" class="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]" />
              <input
                v-model="scanSearch"
                type="text"
                placeholder="搜索技能名称或路径…"
                class="h-7 w-full rounded-lg border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] pl-7 pr-2.5 text-xs text-[var(--ui-text)] outline-none transition focus:border-[var(--ui-accent)]"
              />
            </div>
          </div>

          <!-- List Items -->
          <div class="max-h-[380px] overflow-y-auto space-y-1.5 pt-2.5 divide-y divide-[var(--ui-line)]/50">
            <div
              v-for="item in filteredScanItems"
              :key="item.path"
              class="pt-2 first:pt-0 pb-1 flex items-start justify-between gap-3 transition hover:bg-[var(--ui-surface-hover)] rounded-xl p-2"
            >
              <div class="min-w-0 flex-1 space-y-1">
                <div class="flex flex-wrap items-center gap-1.5">
                  <span :class="providerBadgeClass(item.provider)" class="rounded-md px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-wider">
                    {{ item.provider }}
                  </span>
                  <span v-if="item.type === 'instruction'" class="rounded bg-sky-500/10 text-sky-600 dark:text-sky-400 px-1.5 py-0.5 text-[10px] font-medium">
                    指令规则
                  </span>
                  <strong class="text-xs font-semibold text-[var(--ui-text)] truncate">{{ item.name }}</strong>
                </div>

                <p v-if="item.description" class="text-[11px] text-[var(--ui-text-secondary)] line-clamp-2 leading-relaxed">
                  {{ item.description }}
                </p>

                <div class="flex items-center gap-1.5 text-[10px] text-[var(--ui-text-muted)]">
                  <code class="font-mono truncate max-w-full rounded bg-[var(--ui-surface-subtle)] px-1.5 py-0.5 text-[10px] text-[var(--ui-text-muted)]" :title="item.path">
                    {{ item.path }}
                  </code>
                </div>
              </div>

              <!-- Quick Copy Button -->
              <BaseButton
                variant="ghost"
                size="sm"
                class="shrink-0 h-7 w-7 !p-0"
                :title="copiedPath === item.path ? '已复制' : '复制完整路径'"
                @click="copyItemPath(item.path)"
              >
                <Check v-if="copiedPath === item.path" :size="12" class="text-[var(--ui-success)]" />
                <Copy v-else :size="12" class="text-[var(--ui-text-muted)]" />
              </BaseButton>
            </div>

            <div v-if="!filteredScanItems.length" class="py-6 text-center text-xs text-[var(--ui-text-muted)]">
              未找到匹配的指令或技能
            </div>
          </div>
        </div>
      </details>
    </div>

    </details>
    <TextAreaField v-model="draft.aiInstructions" label="工作区 AI 指令" :rows="5" placeholder="例如：修改代码必须遵循 Clean Code；运行测试后再确认完成。" />
    <div class="ui-inset p-3"><ToggleSwitch v-model="draft.workspaceLocalEntries" label="允许执行 Workspace 内本地入口" description="允许通过本地相对路径直接执行工作区内部工具与脚本" /></div>
    <TextField v-model="draft.workspaceScriptExtensions" label="本地脚本扩展名（逗号分隔）" placeholder=".exe,.bat,.cmd,.ps1" :disabled="!draft.workspaceLocalEntries" />
    <SaveBar :dirty="dirty" :busy="saving" :error="scanError" label="保存策略" @save="save"/>
  </form></fieldset>
</template>

<style scoped>
.runtime-two-col {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: .75rem;
  min-width: 0;
}

@container workspace-page (max-width: 700px) {
  .runtime-two-col {
    grid-template-columns: 1fr;
  }
}
</style>
