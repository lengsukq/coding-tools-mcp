<script lang="ts">
  import { onMount } from "svelte";
  import { ask, message } from "@tauri-apps/plugin-dialog";
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
  import GeneralAboutPanel from "$lib/components/settings/GeneralAboutPanel.svelte";
  import GeneralMemoryPanel from "$lib/components/settings/GeneralMemoryPanel.svelte";
  import GeneralProxyPanel from "$lib/components/settings/GeneralProxyPanel.svelte";
  import GeneralRuntimePanel from "$lib/components/settings/GeneralRuntimePanel.svelte";
  import { showToast } from "$lib/stores/toast";
  import { reloadUiOnly } from "$lib/ui-memory-guard";

  let proxy = $state<ProxyConfigDto>({ mode: "none", url: "" });
  let runtime = $state<GlobalRuntimeSettingsDto>({
    executablePaths: "",
    aiInstructions: "",
    instructionSources: [],
    skillSources: [],
    customInstructionPaths: "",
    customSkillPaths: "",
    allowLanAccess: false,
    restoreRuntimeStateOnLaunch: false,
  });
  let proxyChanged = $state(false);
  let runtimeChanged = $state(false);
  let proxySaving = $state(false);
  let runtimeSaving = $state(false);
  let checkingUpdate = $state(false);
  let releasingUi = $state(false);
  let memoryHint = $state<string | null>(null);
  let globalAgentScan = $state<GlobalAgentContextScanDto | null>(null);
  let scanningGlobalAgents = $state(false);
  let globalAgentScanError = $state("");

  function arraysEqual(a: string[], b: string[]): boolean {
    return a.length === b.length && a.every((value, index) => value === b[index]);
  }

  function applyDetectedGlobalSources() {
    if (!globalAgentScan) return;
    const instructions = [...globalAgentScan.detectedInstructionSources];
    const skills = [...globalAgentScan.detectedSkillSources];
    if (
      arraysEqual(runtime.instructionSources, instructions)
      && arraysEqual(runtime.skillSources, skills)
    ) return;
    runtime.instructionSources = instructions;
    runtime.skillSources = skills;
    runtimeChanged = true;
  }

  async function scanGlobalSources(autoSelectEmpty: boolean) {
    if (scanningGlobalAgents) return;
    scanningGlobalAgents = true;
    globalAgentScanError = "";
    try {
      globalAgentScan = await scanGlobalAgentContext();
      if (!autoSelectEmpty || !globalAgentScan) return;
      let changedByScan = false;
      if (runtime.instructionSources.length === 0 && globalAgentScan.detectedInstructionSources.length > 0) {
        runtime.instructionSources = [...globalAgentScan.detectedInstructionSources];
        changedByScan = true;
      }
      if (runtime.skillSources.length === 0 && globalAgentScan.detectedSkillSources.length > 0) {
        runtime.skillSources = [...globalAgentScan.detectedSkillSources];
        changedByScan = true;
      }
      if (changedByScan) runtimeChanged = true;
    } catch (error) {
      globalAgentScanError = String(error);
    } finally {
      scanningGlobalAgents = false;
    }
  }

  async function refresh() {
    try {
      [proxy, runtime] = await Promise.all([getProxy(), getGlobalRuntimeSettings()]);
      proxyChanged = false;
      runtimeChanged = false;
      await scanGlobalSources(true);
    } catch (error) {
      await message(String(error), { title: "加载失败", kind: "error" });
    }
  }

  async function saveProxy() {
    proxySaving = true;
    try {
      await setProxy(proxy);
      proxyChanged = false;
      await message("代理设置已保存。", { title: "已保存", kind: "info" });
    } catch (error) {
      await message(String(error), { title: "保存失败", kind: "error" });
    } finally {
      proxySaving = false;
    }
  }

  async function saveRuntime() {
    runtimeSaving = true;
    try {
      await setGlobalRuntimeSettings(runtime);
      runtimeChanged = false;
      await message(
        "全局 Runtime 设置已保存。运行状态恢复开关会在下次启动应用时生效；其他 Runtime 配置需要重启对应服务后生效。",
        { title: "已保存", kind: "info" },
      );
    } catch (error) {
      await message(String(error), { title: "保存失败", kind: "error" });
    } finally {
      runtimeSaving = false;
    }
  }

  async function openLink(url: string, title: string) {
    try {
      await openUrl(url);
    } catch (error) {
      await message(String(error), { title, kind: "error" });
    }
  }

  async function handleCheckUpdate() {
    if (checkingUpdate) return;
    checkingUpdate = true;
    try {
      const result = await checkAppUpdate();
      if (!result.updateAvailable) {
        await message(`当前已是最新版本（v${result.currentVersion}）。`, {
          title: "检查更新",
          kind: "info",
        });
        return;
      }
      const shouldOpen = await ask(
        `发现新版本 ${result.latestTag}（当前 v${result.currentVersion}）。是否打开 Releases 页面下载？`,
        { title: "有可用更新", kind: "info", okLabel: "打开下载页", cancelLabel: "稍后" },
      );
      if (shouldOpen) await openUrl(result.releaseUrl || RELEASES_LATEST_URL);
    } catch (error) {
      await message(String(error), { title: "检查更新失败", kind: "error" });
    } finally {
      checkingUpdate = false;
    }
  }

  async function refreshMemoryHint() {
    try {
      const sample = await getWebviewMemorySample();
      if (!sample.supported) {
        memoryHint = "当前平台暂不支持界面内存采样。";
        return;
      }
      memoryHint = `界面约 ${Math.round(sample.webviewMb)} MB（${sample.webviewProcessCount} 个 WebView 进程），主进程约 ${Math.round(sample.mainMb)} MB。`;
    } catch {
      memoryHint = null;
    }
  }

  async function handleReleaseUiMemory() {
    if (releasingUi) return;
    const confirmed = await ask(
      "将重建界面进程（WebView）以释放内存。MCP 与 FRP 隧道会继续在后台运行，不会被停止。",
      { title: "释放界面内存", kind: "info", okLabel: "立即释放", cancelLabel: "取消" },
    );
    if (!confirmed) return;
    releasingUi = true;
    showToast("正在重建界面进程…", { title: "释放界面内存", kind: "info", duration: 2000 });
    await reloadUiOnly("settings-manual");
  }

  onMount(() => {
    void refresh();
    void refreshMemoryHint();
  });
</script>

<section class="page-scroll">
  <header class="page-header">
    <p class="page-kicker">全局设置</p>
    <h2 class="page-title">通用</h2>
    <p class="mt-2 max-w-2xl text-sm text-[var(--color-text-muted)]">
      配置全局 Agent Runtime、网络代理，并查看应用版本与官方仓库入口。
    </p>
  </header>

  <div class="page-body flex flex-col gap-6">
    <GeneralAboutPanel
      version={APP_VERSION}
      {checkingUpdate}
      onOpenRepo={() => openLink(REPO_URL, "无法打开仓库")}
      onOpenReleases={() => openLink(RELEASES_LATEST_URL, "无法打开 Releases")}
      onCheckUpdate={handleCheckUpdate}
    />
    <GeneralMemoryPanel
      {memoryHint}
      releasing={releasingUi}
      onRefresh={refreshMemoryHint}
      onRelease={handleReleaseUiMemory}
    />
    <GeneralRuntimePanel
      {runtime}
      changed={runtimeChanged}
      saving={runtimeSaving}
      scan={globalAgentScan}
      scanning={scanningGlobalAgents}
      scanError={globalAgentScanError}
      onChange={() => { runtimeChanged = true; }}
      onSave={saveRuntime}
      onScan={() => scanGlobalSources(false)}
      onApplyDetected={applyDetectedGlobalSources}
    />
    <GeneralProxyPanel
      {proxy}
      changed={proxyChanged}
      saving={proxySaving}
      onChange={() => { proxyChanged = true; }}
      onSave={saveProxy}
    />
  </div>
</section>
