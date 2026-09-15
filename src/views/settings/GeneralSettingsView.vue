<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { ask } from "@tauri-apps/plugin-dialog";
import { ExternalLink, MemoryStick, RefreshCw, Save, ScanSearch } from "@lucide/vue";
import BaseButton from "../../components/ui/BaseButton.vue";
import GlassCard from "../../components/ui/GlassCard.vue";
import TextField from "../../components/ui/TextField.vue";
import ToggleSwitch from "../../components/ui/ToggleSwitch.vue";
import SettingsPageHeader from "../../components/settings/SettingsPageHeader.vue";
import { checkAppUpdate, openUrl } from "$lib/api/app-info";
import { scanGlobalAgentContext, type GlobalAgentContextScanDto } from "$lib/api/agent-context";
import {
  getGlobalRuntimeSettings,
  getProxy,
  setGlobalRuntimeSettings,
  setProxy,
  type GlobalRuntimeSettingsDto,
  type ProxyConfigDto,
} from "$lib/api/settings";
import { getWebviewMemorySample } from "$lib/api/ui-memory";
import { RELEASES_LATEST_URL, REPO_URL } from "$lib/app-links";
import { APP_VERSION } from "$lib/app-version";
import { AGENT_SOURCE_OPTIONS, toggleSource } from "$lib/agent-context";
import { showToast } from "$lib/stores/toast";
import { reloadUiOnly } from "$lib/ui-memory-guard";

const proxy = reactive<ProxyConfigDto>({ mode: "none", url: "" });
const runtime = reactive<GlobalRuntimeSettingsDto>({
  executablePaths: "", aiInstructions: "", instructionSources: [], skillSources: [], customInstructionPaths: "", customSkillPaths: "", allowLanAccess: false, restoreRuntimeStateOnLaunch: false,
});
const loading = ref(true);
const proxySaving = ref(false);
const runtimeSaving = ref(false);
const checkingUpdate = ref(false);
const releasingUi = ref(false);
const memoryHint = ref<string | null>(null);
const scan = ref<GlobalAgentContextScanDto | null>(null);
const scanning = ref(false);

async function refresh() {
  loading.value = true;
  try {
    const [nextProxy, nextRuntime] = await Promise.all([getProxy(), getGlobalRuntimeSettings()]);
    Object.assign(proxy, nextProxy);
    Object.assign(runtime, nextRuntime);
    await scanSources(true);
    await refreshMemory();
  } catch (error) { showToast(String(error), { title: "加载设置失败", kind: "error" }); }
  finally { loading.value = false; }
}

async function scanSources(autoApply = false) {
  scanning.value = true;
  try {
    scan.value = await scanGlobalAgentContext();
    if (autoApply && scan.value) {
      if (runtime.instructionSources.length === 0) runtime.instructionSources = [...scan.value.detectedInstructionSources];
      if (runtime.skillSources.length === 0) runtime.skillSources = [...scan.value.detectedSkillSources];
    }
  } catch (error) { showToast(String(error), { title: "扫描 Agent Context 失败", kind: "warning" }); }
  finally { scanning.value = false; }
}

function applyDetected() {
  if (!scan.value) return;
  runtime.instructionSources = [...scan.value.detectedInstructionSources];
  runtime.skillSources = [...scan.value.detectedSkillSources];
}

async function saveProxy() {
  proxySaving.value = true;
  try { await setProxy({ ...proxy }); showToast("代理设置已保存", { kind: "success" }); }
  catch (error) { showToast(String(error), { title: "保存代理失败", kind: "error" }); }
  finally { proxySaving.value = false; }
}

async function saveRuntime() {
  runtimeSaving.value = true;
  try {
    await setGlobalRuntimeSettings({ ...runtime, instructionSources: [...runtime.instructionSources], skillSources: [...runtime.skillSources] });
    showToast("全局 Runtime 设置已保存；运行中的服务需重启后应用。", { kind: "success", duration: 7000 });
  } catch (error) { showToast(String(error), { title: "保存 Runtime 设置失败", kind: "error" }); }
  finally { runtimeSaving.value = false; }
}

async function checkUpdate() {
  checkingUpdate.value = true;
  try {
    const result = await checkAppUpdate();
    if (!result.updateAvailable) { showToast(`当前已是最新版本 v${result.currentVersion}`, { kind: "success" }); return; }
    const openRelease = await ask(`发现新版本 ${result.latestTag}，是否打开 Releases？`, { title: "有可用更新", kind: "info", okLabel: "打开下载页", cancelLabel: "稍后" });
    if (openRelease) await openUrl(result.releaseUrl || RELEASES_LATEST_URL);
  } catch (error) { showToast(String(error), { title: "检查更新失败", kind: "error" }); }
  finally { checkingUpdate.value = false; }
}

async function refreshMemory() {
  try {
    const sample = await getWebviewMemorySample();
    memoryHint.value = sample.supported ? `界面约 ${Math.round(sample.webviewMb)} MB（${sample.webviewProcessCount} 个 WebView），主进程约 ${Math.round(sample.mainMb)} MB。` : "当前平台暂不支持界面内存采样。";
  } catch { memoryHint.value = null; }
}

async function releaseUi() {
  const confirmed = await ask("将重建 WebView 释放界面内存，MCP 与隧道服务不会停止。", { title: "释放界面内存", kind: "info", okLabel: "立即释放", cancelLabel: "取消" });
  if (!confirmed) return;
  releasingUi.value = true;
  await reloadUiOnly("settings-manual");
}

onMounted(() => { void refresh(); });
</script>

<template>
  <div class="mx-auto max-w-[1180px] px-8 py-7">
    <SettingsPageHeader title="通用" description="配置全局 Agent Runtime、网络代理、自动恢复与界面运行状态。" />
    <div class="grid gap-4">
      <GlassCard>
        <div class="flex items-center gap-4"><div class="flex h-11 w-11 items-center justify-center rounded-2xl bg-gradient-to-br from-[#0a84ff]/16 to-[#5e5ce6]/12 text-[#0a84ff]"><span class="text-xs font-black">CT</span></div><div class="min-w-0 flex-1"><h2 class="text-sm font-semibold">Coding Tools MCP</h2><p class="mt-1 text-xs text-[var(--text-muted)]">v{{ APP_VERSION }} · Tauri 2 · Vue 3 · Tailwind CSS 4</p></div><BaseButton variant="ghost" @click="openUrl(REPO_URL)"><ExternalLink :size="14" />仓库</BaseButton><BaseButton variant="secondary" :busy="checkingUpdate" @click="checkUpdate"><RefreshCw :size="14" />检查更新</BaseButton></div>
      </GlassCard>

      <GlassCard>
        <div class="mb-4 flex items-center justify-between"><div><h2 class="text-sm font-semibold">界面内存</h2><p class="mt-1 text-xs text-[var(--text-muted)]">{{ memoryHint || '正在读取内存信息…' }}</p></div><MemoryStick :size="18" class="text-[#bf5af2]" /></div>
        <div class="flex justify-end gap-2"><BaseButton variant="ghost" size="sm" @click="refreshMemory"><RefreshCw :size="13" />刷新</BaseButton><BaseButton variant="secondary" size="sm" :busy="releasingUi" @click="releaseUi">释放界面内存</BaseButton></div>
      </GlassCard>

      <GlassCard>
        <div class="mb-5 flex items-center justify-between"><div><h2 class="text-sm font-semibold">全局 Runtime</h2><p class="mt-1 text-xs text-[var(--text-muted)]">统一定义 Agent Context 来源、执行路径和启动行为。</p></div><div class="flex gap-2"><BaseButton variant="ghost" size="sm" :busy="scanning" @click="scanSources(false)"><ScanSearch :size="13" />扫描</BaseButton><BaseButton v-if="scan" variant="ghost" size="sm" @click="applyDetected">应用检测结果</BaseButton></div></div>
        <div class="grid gap-4">
          <div class="grid grid-cols-2 gap-3"><TextField v-model="runtime.executablePaths" label="额外可执行路径" /><TextField v-model="runtime.customInstructionPaths" label="自定义 Instructions 路径" /></div>
          <TextField v-model="runtime.customSkillPaths" label="自定义 Skills 路径" />
          <label><span class="mb-1.5 block text-xs font-semibold text-[var(--text-secondary)]">全局 AI Instructions</span><textarea v-model="runtime.aiInstructions" rows="4" class="w-full resize-y rounded-xl border border-white/70 bg-white/60 p-3 text-sm outline-none focus:border-[#0a84ff]/50 dark:border-white/10 dark:bg-white/6" /></label>
          <div class="grid grid-cols-2 gap-3">
            <div class="rounded-2xl bg-black/[.025] p-3 dark:bg-white/[.04]"><p class="mb-2 text-xs font-semibold">Instruction 来源</p><label v-for="item in AGENT_SOURCE_OPTIONS" :key="`i-${item.value}`" class="flex items-center gap-2 py-1 text-xs"><input type="checkbox" class="accent-[#0a84ff]" :checked="runtime.instructionSources.includes(item.value)" @change="runtime.instructionSources = toggleSource(runtime.instructionSources, item.value, ($event.target as HTMLInputElement).checked)" />{{ item.label }}</label></div>
            <div class="rounded-2xl bg-black/[.025] p-3 dark:bg-white/[.04]"><p class="mb-2 text-xs font-semibold">Skill 来源</p><label v-for="item in AGENT_SOURCE_OPTIONS" :key="`s-${item.value}`" class="flex items-center gap-2 py-1 text-xs"><input type="checkbox" class="accent-[#bf5af2]" :checked="runtime.skillSources.includes(item.value)" @change="runtime.skillSources = toggleSource(runtime.skillSources, item.value, ($event.target as HTMLInputElement).checked)" />{{ item.label }}</label></div>
          </div>
          <div class="grid grid-cols-2 gap-3"><ToggleSwitch v-model="runtime.allowLanAccess" label="允许局域网访问" description="允许本机 Runtime 监听可被 LAN 访问的地址。" /><ToggleSwitch v-model="runtime.restoreRuntimeStateOnLaunch" label="启动时恢复服务" description="应用下次启动时恢复上次运行中的 Workspace。" /></div>
          <div class="flex justify-end"><BaseButton :busy="runtimeSaving" @click="saveRuntime"><Save :size="14" />保存 Runtime</BaseButton></div>
        </div>
      </GlassCard>

      <GlassCard>
        <div class="mb-4"><h2 class="text-sm font-semibold">网络代理</h2><p class="mt-1 text-xs text-[var(--text-muted)]">供下载、Tunnel 等需要访问公网的模块使用。</p></div>
        <div class="grid grid-cols-[220px_minmax(0,1fr)] gap-3"><label><span class="mb-1.5 block text-xs font-semibold text-[var(--text-secondary)]">代理模式</span><select v-model="proxy.mode" class="h-10 w-full rounded-xl border border-white/70 bg-white/60 px-3 text-sm outline-none dark:border-white/10 dark:bg-[#232329]"><option value="none">不使用代理</option><option value="system">系统代理</option><option value="custom">自定义代理</option></select></label><TextField v-model="proxy.url" label="代理 URL" placeholder="http://127.0.0.1:7890" :disabled="proxy.mode !== 'custom'" /></div>
        <div class="mt-4 flex justify-end"><BaseButton :busy="proxySaving" @click="saveProxy"><Save :size="14" />保存代理</BaseButton></div>
      </GlassCard>
    </div>
  </div>
</template>
