<script setup lang="ts">
import { computed, nextTick, onMounted, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import PageShell from "$src/components/ui/PageShell.vue";
import PageHeader from "$src/components/ui/PageHeader.vue";
import PageTabs from "$src/components/ui/PageTabs.vue";
import GeneralSettingsView from "./GeneralSettingsView.vue";
import GatewaySettingsView from "./GatewaySettingsView.vue";
import ComputerSettingsView from "./ComputerSettingsView.vue";
import KeysSettingsView from "./KeysSettingsView.vue";
import FrpSettingsView from "./FrpSettingsView.vue";
import SoftwareSettingsView from "./SoftwareSettingsView.vue";
import { SETTINGS_NAV } from "$lib/ui-model";
const route = useRoute();
const router = useRouter();
const section = computed(() => String(route.params.section ?? 'general'));
async function reveal() { await nextTick(); const panel = String(route.query.panel ?? ''); const element = document.getElementById('settings-' + panel); if (element instanceof HTMLDetailsElement)
    element.open = true; element?.scrollIntoView({ block: 'start' }); }
onMounted(() => void reveal());
watch([section, () => route.query.panel], () => void reveal());
</script>
<template><section class="page-scroll">
<PageShell reading>
<PageHeader kicker="应用级配置" title="应用设置" description="全局连接由所有工作区共享；项目专属配置请进入工作区设置。"/>
<PageTabs :items="SETTINGS_NAV" :model-value="section" aria-label="应用设置分类" @update:model-value="router.push('/settings/'+$event)">
<div :key="section" class="ui-stack">
<template v-if="section==='connection'">
<GatewaySettingsView/>
<GeneralSettingsView section="connection"/>
<details id="settings-keys" class="ui-disclosure">
<summary>认证密钥 <span class="ui-muted">影响全部工作区连接</span>
</summary>
<div class="ui-disclosure-body">
<KeysSettingsView/>
</div>
</details>
<details id="settings-frp" class="ui-disclosure">
<summary>FRP 服务器配置</summary>
<div class="ui-disclosure-body">
<FrpSettingsView/>
</div>
</details>
</template>
<template v-else-if="section==='tools'">
<GeneralSettingsView section="tools"/>
<SoftwareSettingsView/>
</template>
<ComputerSettingsView v-else-if="section==='computer'"/>
<GeneralSettingsView v-else :section="section"/>
</div>
</PageTabs>
</PageShell>
</section></template>
