<script setup lang="ts">
import { computed, ref } from "vue";
import { ChevronDown, ChevronRight } from "@lucide/vue";
import BaseButton from "$src/components/ui/BaseButton.vue";
import GlassCard from "$src/components/ui/GlassCard.vue";
import SaveBar from "$src/components/ui/SaveBar.vue";
import TextAreaField from "$src/components/ui/TextAreaField.vue";
import TextField from "$src/components/ui/TextField.vue";
import AgentContextPicker from "./AgentContextPicker.vue";
import { AGENT_SOURCE_OPTIONS, isSourceRestricted, toggleSource } from "$lib/agent-context";
import type { GlobalAgentContextScanDto } from "$lib/api/agent-context";
import type { GlobalRuntimeSettingsDto } from "$lib/api/settings";

const props = defineProps<{
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
  save: [];
}>();

const showAdvancedSources = ref(false);

const hasRestrictedSources = computed(
  () =>
    isSourceRestricted(props.runtime.instructionSources) ||
    isSourceRestricted(props.runtime.skillSources),
);

function resetToAutoScan() {
  emit("update-runtime", {
    instructionSources: ["auto"],
    skillSources: ["auto"],
  });
}
</script>

<template>
  <GlassCard>
    <div class="ui-section-heading">
      <div>
        <h2>全局 AI 指令与 Skills</h2>
        <p>直接展示本机扫描到的 Skills 与 Agents 指令，按需勾选是否注入到 MCP 会话。</p>
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

      <AgentContextPicker
        :instructions="scan?.instructions ?? []"
        :skills="scan?.skills ?? []"
        :disabled-instruction-ids="runtime.disabledInstructionIds"
        :disabled-skill-ids="runtime.disabledSkillIds"
        :scanning="scanning"
        :has-scanned="scan !== null"
        @update:disabled-instruction-ids="emit('update-runtime', { disabledInstructionIds: $event })"
        @update:disabled-skill-ids="emit('update-runtime', { disabledSkillIds: $event })"
        @rescan="emit('scan')"
      />

      <!-- 高级选项：默认收起，隐藏客户端 Harness 列表与自定义路径 -->
      <div class="ui-inset overflow-hidden">
        <div class="flex flex-wrap items-center justify-between gap-2 px-3.5 py-2.5">
          <button
            type="button"
            class="flex cursor-pointer items-center gap-1.5 text-xs font-medium text-[var(--ui-text-secondary)] transition hover:text-[var(--ui-text)]"
            :aria-expanded="showAdvancedSources"
            @click="showAdvancedSources = !showAdvancedSources"
          >
            <ChevronDown v-if="showAdvancedSources" :size="14" />
            <ChevronRight v-else :size="14" />
            <span>高级：自定义路径与来源目录筛选</span>
          </button>

          <div v-if="hasRestrictedSources" class="flex items-center gap-2">
            <span class="rounded-full border border-amber-500/25 bg-amber-500/10 px-2 py-0.5 text-[10px] font-medium text-amber-700 dark:text-amber-300">
              当前仅注入部分来源
            </span>
            <BaseButton variant="ghost" size="sm" @click="resetToAutoScan">
              恢复自动扫描全部
            </BaseButton>
          </div>
        </div>

        <div v-if="showAdvancedSources" class="space-y-4 border-t border-[var(--ui-line)] p-4">
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

          <div class="ui-two-col border-t border-[var(--ui-line)] pt-3">
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
        </div>
      </div>

      <SaveBar :dirty="dirty" :busy="saving" :error="error" @save="emit('save')" />
    </div>
  </GlassCard>
</template>
