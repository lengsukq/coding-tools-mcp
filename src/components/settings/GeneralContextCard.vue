<script setup lang="ts">
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import SaveBar from "$src/components/ui/SaveBar.vue";
import TextAreaField from "$src/components/ui/TextAreaField.vue";
import TextField from "$src/components/ui/TextField.vue";
import { AGENT_SOURCE_OPTIONS, toggleSource } from "$lib/agent-context";
import type { GlobalAgentContextScanDto } from "$lib/api/agent-context";
import type { GlobalRuntimeSettingsDto } from "$lib/api/settings";

defineProps<{
  runtime: GlobalRuntimeSettingsDto;
  scan: GlobalAgentContextScanDto | null;
  scanning: boolean;
  dirty: boolean;
  saving: boolean;
  error: string;
}>();

const emit = defineEmits<{
  "update-runtime": [patch: Partial<GlobalRuntimeSettingsDto>];
  scan: [];
  "apply-detected": [];
  save: [];
}>();
</script>

<template>
  <GlassCard>
    <div class="ui-section-heading">
      <div>
        <h2>全局 AI 指令与技能来源</h2>
        <p>统一配置全局系统提示词，并自动识别本机 Claude、Cursor、Codex 等客户端的规则与技能目录。</p>
      </div>
    </div>
    <div class="ui-stack">
      <TextAreaField
        :model-value="runtime.aiInstructions"
        label="全局 AI 指令"
        :rows="4"
        placeholder="例如：遵循 Clean Code 规范，完成修改后运行自动化校验。"
        @update:model-value="emit('update-runtime', { aiInstructions: $event })"
      />

      <div class="ui-inset space-y-4 p-4">
        <div class="ui-two-col">
          <div>
            <h3 class="mb-2 text-xs font-semibold tracking-wider text-[var(--ui-text-muted)]">指令来源（Instructions）</h3>
            <label v-for="item in AGENT_SOURCE_OPTIONS" :key="item.value" class="ui-checkbox-row text-xs">
              <input
                type="checkbox"
                :checked="runtime.instructionSources.includes(item.value)"
                @change="emit('update-runtime', { instructionSources: toggleSource(runtime.instructionSources, item.value, ($event.target as HTMLInputElement).checked) })"
              />
              <span>{{ item.label }}</span>
            </label>
          </div>
          <div>
            <h3 class="mb-2 text-xs font-semibold tracking-wider text-[var(--ui-text-muted)]">技能来源（Skills）</h3>
            <label v-for="item in AGENT_SOURCE_OPTIONS" :key="item.value" class="ui-checkbox-row text-xs">
              <input
                type="checkbox"
                :checked="runtime.skillSources.includes(item.value)"
                @change="emit('update-runtime', { skillSources: toggleSource(runtime.skillSources, item.value, ($event.target as HTMLInputElement).checked) })"
              />
              <span>{{ item.label }}</span>
            </label>
          </div>
        </div>

        <div class="ui-two-col">
          <TextField
            :model-value="runtime.customInstructionPaths"
            label="自定义指令路径"
            placeholder="docs/AI_RULES.md"
            @update:model-value="emit('update-runtime', { customInstructionPaths: $event })"
          />
          <TextField
            :model-value="runtime.customSkillPaths"
            label="自定义技能路径"
            placeholder="~/.agents/skills"
            @update:model-value="emit('update-runtime', { customSkillPaths: $event })"
          />
        </div>

        <div class="ui-actions">
          <BaseButton variant="secondary" size="sm" :busy="scanning" @click="emit('scan')">扫描本机来源</BaseButton>
          <BaseButton v-if="scan" variant="ghost" size="sm" @click="emit('apply-detected')">应用检测结果</BaseButton>
          <span v-if="scan" class="ui-muted">
            已检测到 {{ scan.detectedInstructionSources.join("、") || "无" }} 指令来源 · {{ scan.detectedSkillSources.join("、") || "无" }} 技能来源
          </span>
        </div>
      </div>

      <SaveBar :dirty="dirty" :busy="saving" :error="error" @save="emit('save')" />
    </div>
  </GlassCard>
</template>
