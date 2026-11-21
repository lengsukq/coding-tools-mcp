<script setup lang="ts">
import GlassCard from "$src/components/ui/GlassCard.vue";
import SaveBar from "$src/components/ui/SaveBar.vue";
import SelectField from "$src/components/ui/SelectField.vue";
import TextField from "$src/components/ui/TextField.vue";
import type { ProxyConfigDto } from "$lib/api/settings";

const PROXY_MODE_OPTIONS = [
  { value: "none", label: "不使用代理" },
  { value: "system", label: "系统代理" },
  { value: "manual", label: "自定义代理" },
];

defineProps<{
  proxy: ProxyConfigDto;
  dirty: boolean;
  saving: boolean;
  error: string;
}>();

const emit = defineEmits<{
  "update-proxy": [patch: Partial<ProxyConfigDto>];
  save: [];
}>();
</script>

<template>
  <GlassCard>
    <div class="ui-section-heading">
      <div>
        <h2>网络代理</h2>
        <p>供公网隧道与需要访问外部网络的模块复用。</p>
      </div>
    </div>
    <div class="ui-two-col">
      <SelectField
        :model-value="proxy.mode"
        label="代理模式"
        :options="PROXY_MODE_OPTIONS"
        @update:model-value="emit('update-proxy', { mode: $event })"
      />
      <TextField
        :model-value="proxy.url"
        label="代理地址"
        :disabled="proxy.mode !== 'manual'"
        placeholder="http://127.0.0.1:7890"
        :error="error"
        @update:model-value="emit('update-proxy', { url: $event })"
      />
    </div>
    <SaveBar :dirty="dirty" :busy="saving" :error="error" @save="emit('save')" />
  </GlassCard>
</template>
