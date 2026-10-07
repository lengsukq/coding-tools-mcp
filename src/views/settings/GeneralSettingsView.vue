<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { ask } from "@tauri-apps/plugin-dialog";
import { ExternalLink, MemoryStick, RefreshCw, Save, ScanSearch } from "@lucide/vue";
import appIcon from "$src/assets/app-icon.png";
import BaseButton from "../../components/ui/BaseButton.vue";
import GlassCard from "../../components/ui/GlassCard.vue";
import SelectField from "../../components/ui/SelectField.vue";
import TextAreaField from "../../components/ui/TextAreaField.vue";
import TextField from "../../components/ui/TextField.vue";
import ToggleSwitch from "../../components/ui/ToggleSwitch.vue";
import SaveBar from "../../components/ui/SaveBar.vue";
import { useUnsavedChanges } from "$lib/unsaved-changes";
import {enqueueRuntimeSettingsWrite} from "$lib/queued-writes";
import { mergeOwnedChanges } from "$lib/runtime-settings-draft";
import PalettePicker from "../../components/settings/PalettePicker.vue";
import { checkAppUpdate, openUrl } from "$lib/api/app-info";
import { scanGlobalAgentContext, type GlobalAgentContextScanDto } from "$lib/api/agent-context";
import { getGlobalRuntimeSettings, getProxy, setGlobalRuntimeSettings, setProxy, type GlobalRuntimeSettingsDto, type ProxyConfigDto, } from "$lib/api/settings";
import { getWebviewMemorySample } from "$lib/api/ui-memory";
import { RELEASES_LATEST_URL, REPO_URL } from "$lib/app-links";
import { APP_VERSION } from "$lib/app-version";
import { AGENT_SOURCE_OPTIONS, toggleSource } from "$lib/agent-context";
import { showToast } from "$lib/stores/toast";
import { reloadUiOnly } from "$lib/ui-memory-guard";
const props = withDefaults(defineProps<{
    section?: string;
}>(), { section: "general" });
const proxy = reactive<ProxyConfigDto>({ mode: "none", url: "" });
const runtime = reactive<GlobalRuntimeSettingsDto>({
    authType: "oauth", executablePaths: "", permissionMode: "trusted", allowedCommands: "", aiInstructions: "", instructionSources: [], skillSources: [], customInstructionPaths: "", customSkillPaths: "", allowLanAccess: false, restoreRuntimeStateOnLaunch: false, migrationNotice: "",
});
const authOptions = [
    { value: "oauth", label: "OAuth（推荐）" },
    { value: "bearer", label: "Bearer Token" },
    { value: "noauth", label: "无认证（仅可信本机环境）" },
];
const permissionOptions = [
    { value: "trusted", label: "受信任（推荐）" },
    { value: "safe", label: "安全受限" },
    { value: "dangerous", label: "完全放开" },
];
const loading = ref(true);
const proxySaving = ref(false);
const runtimeSaving = ref(false);
const checkingUpdate = ref(false);
const releasingUi = ref(false);
const memoryHint = ref<string | null>(null);
const scan = ref<GlobalAgentContextScanDto | null>(null);
const scanning = ref(false);
const loadError = ref("");
const runtimeError = ref("");
const proxyError = ref("");
const runtimeBaseline = ref<GlobalRuntimeSettingsDto | null>(null);
const proxyBaseline = ref("");
const owned = computed<Array<keyof GlobalRuntimeSettingsDto>>(() => props.section === "connection" ? ["authType", "allowLanAccess"] : props.section === "general" ? ["restoreRuntimeStateOnLaunch"] : ["permissionMode", "allowedCommands", "executablePaths", "aiInstructions", "instructionSources", "skillSources", "customInstructionPaths", "customSkillPaths"]);
const runtimeDirty = computed(() => !loading.value && !!runtimeBaseline.value && owned.value.some(key => JSON.stringify(runtime[key]) !== JSON.stringify(runtimeBaseline.value![key])));
const proxyDirty = computed(() => !loading.value && props.section === "tools" && JSON.stringify(proxy) !== proxyBaseline.value);
async function refresh() {
    loading.value = true;
    loadError.value = "";
    try {
        const [nextProxy, nextRuntime] = await Promise.all([getProxy(), getGlobalRuntimeSettings()]);
        Object.assign(proxy, nextProxy, { mode: nextProxy.mode === "custom" ? "manual" : nextProxy.mode });
        Object.assign(runtime, nextRuntime);
        runtimeBaseline.value = JSON.parse(JSON.stringify(runtime));
        proxyBaseline.value = JSON.stringify(proxy);
        if (props.section === "execution")
            await scanSources();
        if (props.section === "about")
            await refreshMemory();
    }
    catch (error) {
        loadError.value = String(error);
    }
    finally {
        loading.value = false;
    }
}
async function scanSources(autoApply = false) {
    scanning.value = true;
    try {
        scan.value = await scanGlobalAgentContext();
        if (autoApply && scan.value) {
            if (runtime.instructionSources.length === 0)
                runtime.instructionSources = [...scan.value.detectedInstructionSources];
            if (runtime.skillSources.length === 0)
                runtime.skillSources = [...scan.value.detectedSkillSources];
        }
    }
    catch (error) {
        showToast(String(error), { title: "扫描 Agent Context 失败", kind: "warning" });
    }
    finally {
        scanning.value = false;
    }
}
function applyDetected() {
    if (!scan.value)
        return;
    runtime.instructionSources = [...scan.value.detectedInstructionSources];
    runtime.skillSources = [...scan.value.detectedSkillSources];
}
async function saveProxy() {
    if (proxySaving.value)
        return false;
    if (!proxyDirty.value)
        return true;
    proxySaving.value = true;
    proxyError.value = "";
    try {
        if (proxy.mode === "manual" && ! /^(https?|socks5h?):\/\//i.test(proxy.url.trim()))
            throw new Error("代理地址须以 http://、https:// 或 socks5:// 开头。");
        await setProxy({ ...proxy });
        proxyBaseline.value = JSON.stringify(proxy);
        showToast("代理设置已保存", { kind: "success" });
        return true;
    }
    catch (e) {
        proxyError.value = String(e);
        return false;
    }
    finally {
        proxySaving.value = false;
    }
}
async function saveRuntime() {
    if (runtimeSaving.value)
        return false;
    if (!runtimeDirty.value || !runtimeBaseline.value)
        return true;
    runtimeSaving.value = true;
    runtimeError.value = "";
    try {
        const draft = JSON.parse(JSON.stringify(runtime)) as GlobalRuntimeSettingsDto;
        const baseline = runtimeBaseline.value;
        const fields = [...owned.value];
        await enqueueRuntimeSettingsWrite(async () => {
            const latest = await getGlobalRuntimeSettings();
            await setGlobalRuntimeSettings(mergeOwnedChanges(latest, draft, baseline, fields));
        });
        runtimeBaseline.value = draft;
        showToast("设置已保存；连接与启动相关更改在重启 MCP 后应用。", { kind: "success" });
        return true;
    }
    catch (e) {
        runtimeError.value = String(e);
        return false;
    }
    finally {
        runtimeSaving.value = false;
    }
}
useUnsavedChanges("全局 " + props.section + " 设置", runtimeDirty, saveRuntime);
useUnsavedChanges("网络代理", proxyDirty, saveProxy);
async function checkUpdate() {
    checkingUpdate.value = true;
    try {
        const result = await checkAppUpdate();
        if (!result.updateAvailable) {
            showToast(`当前已是最新版本 v${result.currentVersion}`, { kind: "success" });
            return;
        }
        const openRelease = await ask(`发现新版本 ${result.latestTag}，是否打开 Releases？`, { title: "有可用更新", kind: "info", okLabel: "打开下载页", cancelLabel: "稍后" });
        if (openRelease)
            await openUrl(result.releaseUrl || RELEASES_LATEST_URL);
    }
    catch (error) {
        showToast(String(error), { title: "检查更新失败", kind: "error" });
    }
    finally {
        checkingUpdate.value = false;
    }
}
async function refreshMemory() {
    try {
        const sample = await getWebviewMemorySample();
        memoryHint.value = sample.supported ? `界面约 ${Math.round(sample.webviewMb)} MB（${sample.webviewProcessCount} 个 WebView），主进程约 ${Math.round(sample.mainMb)} MB。` : "当前平台暂不支持界面内存采样。";
    }
    catch {
        memoryHint.value = null;
    }
}
async function releaseUi() {
    const confirmed = await ask("将重建 WebView 释放界面内存，MCP 与隧道服务不会停止。", { title: "释放界面内存", kind: "info", okLabel: "立即释放", cancelLabel: "取消" });
    if (!confirmed)
        return;
    releasingUi.value = true;
    await reloadUiOnly("settings-manual");
}
onMounted(() => { void refresh(); });
</script>

<template><div class="ui-stack">
 <p v-if="loading" role="status" class="ui-muted">正在加载设置…</p>
 <div v-else-if="loadError" class="ui-banner ui-banner--danger" role="alert">
<span>设置加载失败：{{loadError}}</span>
<BaseButton variant="secondary" @click="refresh">重试</BaseButton>
</div>
 <fieldset v-else :disabled="runtimeSaving||proxySaving" class="ui-stack">
 <template v-if="section==='general'">
<GlassCard>
<div class="ui-section-heading">
<div>
<h2>外观</h2>
<p>品牌强调色独立于状态颜色，浅色与深色界面保持一致。</p>
</div>
</div>
<PalettePicker/>
</GlassCard>
<GlassCard>
<div class="ui-section-heading">
<h2>启动行为</h2>
</div>
<ToggleSwitch v-model="runtime.restoreRuntimeStateOnLaunch" label="启动时恢复 MCP 服务" description="影响应用下次启动；所有工作区共享同一个 MCP 服务。"/>
<SaveBar :dirty="runtimeDirty" :busy="runtimeSaving" :error="runtimeError" @save="saveRuntime"/>
</GlassCard>
</template>
 <GlassCard v-else-if="section==='connection'">
<div class="ui-section-heading">
<div>
<h2>认证与访问范围</h2>
<p>影响全部工作区的连接；保存后可能需要重启 MCP。</p>
</div>
</div>
<div class="ui-stack">
<SelectField v-model="runtime.authType" label="认证模式" :options="authOptions"/>
<ToggleSwitch v-model="runtime.allowLanAccess" label="允许局域网访问" description="允许局域网设备访问唯一 MCP 端点。"/>
<p v-if="runtime.migrationNotice" class="ui-banner ui-banner--warning">{{runtime.migrationNotice}}</p>
<SaveBar :dirty="runtimeDirty" :busy="runtimeSaving" :error="runtimeError" @save="saveRuntime"/>
</div>
</GlassCard>
 <template v-else-if="section==='execution'">
<GlassCard>
<div class="ui-section-heading">
<div>
<h2>全局执行默认值</h2>
<p>作为新工作区默认值；已选择继承的工作区使用全局权限，单独覆盖的工作区保持其配置。</p>
</div>
</div>
<div class="ui-stack">
<SelectField v-model="runtime.permissionMode" label="默认权限模式" :options="permissionOptions"/>
<details class="ui-disclosure">
<summary>命令白名单与执行路径</summary>
<div class="ui-disclosure-body">
<TextAreaField v-model="runtime.allowedCommands" label="默认系统命令（逗号分隔）" :rows="4"/>
<TextAreaField v-model="runtime.executablePaths" label="默认执行路径" :rows="5" mono/>
</div>
</details>
<TextAreaField v-model="runtime.aiInstructions" label="全局 AI 指令" :rows="4"/>
<details class="ui-disclosure">
<summary>指令与技能来源</summary>
<div class="ui-disclosure-body">
<div class="ui-two-col">
<div>
<h3>指令来源</h3>
<label v-for="item in AGENT_SOURCE_OPTIONS" :key="item.value" class="ui-checkbox-row">
<input type="checkbox" :checked="runtime.instructionSources.includes(item.value)" @change="runtime.instructionSources=toggleSource(runtime.instructionSources,item.value,($event.target as HTMLInputElement).checked)"/>{{item.label}}</label>
</div>
<div>
<h3>技能来源</h3>
<label v-for="item in AGENT_SOURCE_OPTIONS" :key="item.value" class="ui-checkbox-row">
<input type="checkbox" :checked="runtime.skillSources.includes(item.value)" @change="runtime.skillSources=toggleSource(runtime.skillSources,item.value,($event.target as HTMLInputElement).checked)"/>{{item.label}}</label>
</div>
</div>
<TextField v-model="runtime.customInstructionPaths" label="自定义指令路径"/>
<TextField v-model="runtime.customSkillPaths" label="自定义技能路径"/>
<div class="ui-actions">
<BaseButton variant="secondary" :busy="scanning" @click="scanSources()">扫描来源</BaseButton>
<BaseButton v-if="scan" variant="ghost" @click="applyDetected">应用检测结果</BaseButton>
</div>
<p v-if="scan" class="ui-muted">检测到 {{scan.detectedInstructionSources.join('、')}} 指令来源及 {{scan.detectedSkillSources.join('、')}} 技能来源</p>
</div>
</details>
<SaveBar :dirty="runtimeDirty" :busy="runtimeSaving" :error="runtimeError" @save="saveRuntime"/>
</div>
</GlassCard>
</template>
 <GlassCard v-else-if="section==='tools'">
<div class="ui-section-heading">
<div>
<h2>网络代理</h2>
<p>供公网隧道与需要访问网络的模块使用。</p>
</div>
</div>
<div class="ui-two-col">
<SelectField v-model="proxy.mode" label="代理模式" :options="[{value:'none',label:'不使用代理'},{value:'system',label:'系统代理'},{value:'manual',label:'自定义代理'}]"/>
<TextField v-model="proxy.url" label="代理地址" :disabled="proxy.mode!=='manual'" placeholder="http://127.0.0.1:7890" :error="proxyError"/>
</div>
<SaveBar :dirty="proxyDirty" :busy="proxySaving" :error="proxyError" @save="saveProxy"/>
</GlassCard>
 <template v-else-if="section==='about'">
<GlassCard>
<div class="ui-section-heading">
<div>
<h2>Coding Tools MCP</h2>
<p>本地 AI 开发工作台 · v{{APP_VERSION}}</p>
</div>
<img :src="appIcon" alt="" class="size-11"/>
</div>
<p class="ui-secondary">管理项目、追溯 AI 修改，并根据验证记录完成任务验收。</p>
<div class="ui-actions mt-4">
<BaseButton variant="secondary" :busy="checkingUpdate" @click="checkUpdate">检查更新</BaseButton>
<BaseButton variant="ghost" @click="openUrl(REPO_URL)">仓库与帮助</BaseButton>
</div>
</GlassCard>
<details class="ui-disclosure">
<summary>高级维护</summary>
<div class="ui-disclosure-body">
<h3>界面内存</h3>
<p>{{memoryHint||'内存信息暂不可用。'}}</p>
<div class="ui-actions">
<BaseButton variant="secondary" @click="refreshMemory">刷新</BaseButton>
<BaseButton variant="ghost" :busy="releasingUi" @click="releaseUi">释放界面内存</BaseButton>
</div>
</div>
</details>
</template>
 </fieldset>
</div></template>
