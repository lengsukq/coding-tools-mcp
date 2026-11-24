<script setup lang="ts">
import GlassCard from "$src/components/ui/GlassCard.vue";
import SaveBar from "$src/components/ui/SaveBar.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import TextAreaField from "$src/components/ui/TextAreaField.vue";
import type { GlobalRuntimeSettingsDto } from "$lib/api/settings";

const PERMISSION_OPTIONS = [
  { value: "trusted", label: "受信任（推荐）" },
  { value: "safe", label: "安全受限" },
  { value: "dangerous", label: "完全放开" },
];

defineProps<{
  runtime: GlobalRuntimeSettingsDto;
  dirty: boolean;
  saving: boolean;
  error: string;
}>();

const emit = defineEmits<{
  "update-runtime": [patch: Partial<GlobalRuntimeSettingsDto>];
  save: [];
}>();
</script>

<template>
  <GlassCard>
    <div class="ui-section-heading">
      <div>
        <h2>默认权限与命令白名单</h2>
        <p>作为新工作区默认值；已开启「继承全局执行权限」的工作区会实时使用此策略。</p>
      </div>
    </div>
    <div class="ui-stack">
      <SelectField
        :model-value="runtime.permissionMode"
        label="默认权限模式"
        :options="PERMISSION_OPTIONS"
        @update:model-value="emit('update-runtime', { permissionMode: $event })"
      />
      <div
        v-if="runtime.permissionMode === 'dangerous'"
        class="ui-banner ui-banner--warning text-xs leading-5"
      >
        完全放开模式允许执行非白名单命令（如 <code>psql</code> / <code>mysql</code>）并通过 <code>env</code> 向子进程注入环境变量；仍保留工作区目录边界、<code>.git</code> 保护与 Shell 注入拦截。
      </div>
      <TextAreaField
        :model-value="runtime.allowedCommands"
        label="默认允许的系统命令（逗号分隔）"
        :rows="4"
        placeholder="git,npm,pnpm,node,cargo,python3,..."
        @update:model-value="emit('update-runtime', { allowedCommands: $event })"
      />
      <TextAreaField
        :model-value="runtime.executablePaths"
        label="默认执行路径（PATH，每行一个目录）"
        :rows="5"
        mono
        placeholder="/opt/homebrew/bin&#10;/usr/local/bin&#10;~/.cargo/bin"
        @update:model-value="emit('update-runtime', { executablePaths: $event })"
      />
      <SaveBar :dirty="dirty" :busy="saving" :error="error" @save="emit('save')" />
    </div>
  </GlassCard>
</template>
