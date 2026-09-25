<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import { ask } from "@tauri-apps/plugin-dialog";
import { ExternalLink, MemoryStick, RefreshCw, Save, ScanSearch } from "@lucide/vue";
import BaseButton from "../../components/ui/BaseButton.vue";
import GlassCard from "../../components/ui/GlassCard.vue";
import SelectField from "../../components/ui/SelectField.vue";
import TextAreaField from "../../components/ui/TextAreaField.vue";
import TextField from "../../components/ui/TextField.vue";
import ToggleSwitch from "../../components/ui/ToggleSwitch.vue";
import SettingsPageHeader from "../../components/settings/SettingsPageHeader.vue";
import PalettePicker from "../../components/settings/PalettePicker.vue";
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
  <div class="settings-page mx-auto max-w-[1180px] px-8 py-7">
    <SettingsPageHeader title="通用" description="配置全局 Agent Runtime、网络代理、自动恢复与界面运行状态。" />
    <div v-if="loading" class="grid gap-4" aria-busy="true" aria-live="polite">
      <GlassCard>
        <div class="flex min-h-28 items-center justify-center gap-3 text-sm text-[var(--text-muted)]">
          <RefreshCw :size="17" class="animate-spin text-[var(--primary)]" />
          <span>正在加载全局设置与 Agent Context…</span>
        </div>
      </GlassCard>
    </div>
    <div v-else class="grid gap-4 animate-fade-in-up delay-100">
      <GlassCard>
        <div class="flex flex-wrap items-center gap-3"><div class="flex h-11 w-11 items-center justify-center rounded-2xl bg-[var(--primary-soft)] text-[var(--primary)]"><span class="text-xs font-black">CT</span></div><div class="min-w-[180px] flex-1"><h2 class="text-sm font-semibold font-display">Coding Tools MCP</h2><p class="mt-1 text-xs text-[var(--text-muted)]">v{{ APP_VERSION }} · Tauri 2 · Vue 3 · Tailwind CSS 4</p></div><BaseButton variant="ghost" @click="openUrl(REPO_URL)"><ExternalLink :size="14" />仓库</BaseButton><BaseButton variant="secondary" :busy="checkingUpdate" @click="checkUpdate"><RefreshCw :size="14" />检查更新</BaseButton></div>
      </GlassCard>

      <GlassCard>
        <div class="mb-4"><h2 class="text-sm font-semibold font-display">外观</h2><p class="mt-1 text-xs text-[var(--text-muted)]">选择应用的主题调色盘，切换后实时生效（深浅模式下会自动适配）。</p></div>
        <PalettePicker />
      </GlassCard>

      <GlassCard>
        <div class="mb-4 flex items-center justify-between"><div><h2 class="text-sm font-semibold font-display">界面内存</h2><p class="mt-1 text-xs text-[var(--text-muted)]">{{ memoryHint || '正在读取内存信息…' }}</p></div><MemoryStick :size="18" class="text-[var(--primary)]" /></div>
        <div class="flex justify-end gap-2"><BaseButton variant="ghost" size="sm" @click="refreshMemory"><RefreshCw :size="13" />刷新</BaseButton><BaseButton variant="secondary" size="sm" :busy="releasingUi" @click="releaseUi">释放界面内存</BaseButton></div>
      </GlassCard>

      <GlassCard>
        <div class="mb-5 flex flex-wrap items-center justify-between gap-3"><div><h2 class="text-sm font-semibold font-display">全局 Runtime</h2><p class="mt-1 text-xs text-[var(--text-muted)]">统一定义 Agent Context 来源、执行路径和启动行为。</p></div><div class="flex flex-wrap gap-2"><BaseButton variant="ghost" size="sm" :busy="scanning" @click="scanSources(false)"><ScanSearch :size="13" />扫描</BaseButton><BaseButton v-if="scan" variant="ghost" size="sm" @click="applyDetected">应用检测结果</BaseButton></div></div>
        <div class="grid gap-4">
          <div v-if="runtime.migrationNotice" class="rounded-2xl border border-[#ff9f0a]/25 bg-[#ff9f0a]/8 px-4 py-3 text-xs leading-5 text-[var(--text-secondary)]">
            <strong class="text-[#c56b00] dark:text-[#ffb340]">0.3.0 迁移需要复核</strong>
            <p class="mt-1 whitespace-pre-line">{{ runtime.migrationNotice }}</p>
            <p class="mt-1 text-[11px] text-[var(--text-muted)]">确认下面的 Global MCP 认证与连接设置后保存，本提示将自动清除。</p>
          </div>
          <div class="ios-glass ios-inset-surface p-4">
            <div class="mb-4"><h3 class="text-xs font-semibold">Global MCP 连接</h3><p class="mt-1 text-[11px] leading-4 text-[var(--text-muted)]">整个应用只暴露一个 MCP Endpoint。所有 Chat 会话通过 Workspace 选择在同一连接内切换项目。</p></div>
            <SelectField v-model="runtime.authType" label="全局认证模式" :options="authOptions" hint="认证属于唯一 Global MCP，不再按 Workspace 单独配置。" />
          </div>
          <div class="ios-glass ios-inset-surface p-4">
            <div class="mb-4"><h3 class="text-xs font-semibold">全局执行权限</h3><p class="mt-1 text-[11px] leading-4 text-[var(--text-muted)]">新建 Workspace 默认继承这里的权限模式与命令白名单。受信任模式允许常规开发和网络命令，但破坏性操作仍保留安全保护。</p></div>
            <div class="grid gap-3">
              <SelectField v-model="runtime.permissionMode" label="默认权限模式" :options="permissionOptions" />
              <TextAreaField v-model="runtime.allowedCommands" label="默认系统命令（逗号分隔）" :rows="4" mono hint="预置常见开发命令；可以继续追加 gh、aws、docker、kubectl 等本机工具。" />
              <TextAreaField v-model="runtime.executablePaths" label="默认可执行 PATH" :rows="6" mono hint="已按当前平台预置常见工具目录。Workspace 额外 PATH 会排在全局 PATH 之前。" />
            </div>
          </div>
          <div class="settings-two-col"><TextField v-model="runtime.customInstructionPaths" label="自定义 Instructions 路径" /><TextField v-model="runtime.customSkillPaths" label="自定义 Skills 路径" /></div>
          <label><span class="mb-1.5 block text-xs font-semibold text-[var(--text-secondary)]">全局 AI Instructions</span><textarea v-model="runtime.aiInstructions" rows="4" class="w-full resize-y rounded-xl border border-white/70 bg-white/60 p-3 text-sm outline-none focus:border-[var(--primary)]/50 dark:border-white/10 dark:bg-white/6" /></label>
          <div class="settings-two-col">
            <div class="rounded-2xl bg-black/[.025] p-3 dark:bg-white/[.04]"><p class="mb-2 text-xs font-semibold">Instruction 来源</p><label v-for="item in AGENT_SOURCE_OPTIONS" :key="`i-${item.value}`" class="flex items-center gap-2 py-1 text-xs"><input type="checkbox" class="accent-[var(--primary)]" :checked="runtime.instructionSources.includes(item.value)" @change="runtime.instructionSources = toggleSource(runtime.instructionSources, item.value, ($event.target as HTMLInputElement).checked)" />{{ item.label }}</label></div>
            <div class="rounded-2xl bg-black/[.025] p-3 dark:bg-white/[.04]"><p class="mb-2 text-xs font-semibold">Skill 来源</p><label v-for="item in AGENT_SOURCE_OPTIONS" :key="`s-${item.value}`" class="flex items-center gap-2 py-1 text-xs"><input type="checkbox" class="accent-[var(--accent-indigo)]" :checked="runtime.skillSources.includes(item.value)" @change="runtime.skillSources = toggleSource(runtime.skillSources, item.value, ($event.target as HTMLInputElement).checked)" />{{ item.label }}</label></div>
          </div>
          <div class="settings-two-col"><ToggleSwitch v-model="runtime.allowLanAccess" label="允许局域网访问" description="允许唯一 Global MCP Endpoint 被局域网设备访问。" /><ToggleSwitch v-model="runtime.restoreRuntimeStateOnLaunch" label="启动时恢复 Global MCP" description="应用下次启动时恢复唯一 MCP 服务，不再逐 Workspace 启停。" /></div>
          <div class="flex justify-end"><BaseButton :busy="runtimeSaving" @click="saveRuntime"><Save :size="14" />保存 Runtime</BaseButton></div>
        </div>
      </GlassCard>

      <GlassCard>
        <div class="mb-4"><h2 class="text-sm font-semibold">网络代理</h2><p class="mt-1 text-xs text-[var(--text-muted)]">供下载、Tunnel 等需要访问公网的模块使用。</p></div>
        <div class="proxy-settings-grid"><label><span class="mb-1.5 block text-xs font-semibold text-[var(--text-secondary)]">代理模式</span><select v-model="proxy.mode" class="h-10 w-full rounded-xl border border-white/70 bg-white/60 px-3 text-sm outline-none dark:border-white/10 dark:bg-[#232329]"><option value="none">不使用代理</option><option value="system">系统代理</option><option value="custom">自定义代理</option></select></label><TextField v-model="proxy.url" label="代理 URL" placeholder="http://127.0.0.1:7890" :disabled="proxy.mode !== 'custom'" /></div>
        <div class="mt-4 flex justify-end"><BaseButton :busy="proxySaving" @click="saveProxy"><Save :size="14" />保存代理</BaseButton></div>
      </GlassCard>
    </div>
  </div>
</template>

<style scoped>
.settings-page {
  width: 100%;
  min-width: 0;
  box-sizing: border-box;
}

.settings-two-col,
.proxy-settings-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: .75rem;
  min-width: 0;
}

.proxy-settings-grid {
  grid-template-columns: minmax(160px, 220px) minmax(0, 1fr);
}

@container app-main (max-width: 700px) {
  .settings-page {
    padding-inline: 16px;
  }

  .settings-two-col,
  .proxy-settings-grid {
    grid-template-columns: 1fr;
  }
}
</style>
