<script lang="ts">
  import { onMount } from "svelte";
  import { ask, message } from "@tauri-apps/plugin-dialog";
  import { ExternalLink, RefreshCw } from "@lucide/svelte";
  import {
    getGlobalRuntimeSettings,
    getProxy,
    setGlobalRuntimeSettings,
    setProxy,
    type GlobalRuntimeSettingsDto,
    type ProxyConfigDto,
  } from "$lib/api/settings";
  import { checkAppUpdate, openUrl } from "$lib/api/app-info";
  import { APP_VERSION } from "$lib/app-version";
  import { RELEASES_LATEST_URL, REPO_URL } from "$lib/app-links";
  import { getWebviewMemorySample } from "$lib/api/ui-memory";
  import { reloadUiOnly } from "$lib/ui-memory-guard";
  import { showToast } from "$lib/stores/toast";
  import { AGENT_SOURCE_OPTIONS, toggleSource } from "$lib/agent-context";
  import {
    scanGlobalAgentContext,
    type AgentSourceDetectionDto,
    type GlobalAgentContextScanDto,
  } from "$lib/api/agent-context";
  import Button from "$lib/components/ui/Button.svelte";
  import Toggle from "$lib/components/ui/Toggle.svelte";
  import Select from "$lib/components/ui/Select.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";

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
  let changed = $state(false);
  let runtimeChanged = $state(false);
  let saving = $state(false);
  let runtimeSaving = $state(false);
  let checkingUpdate = $state(false);
  let releasingUi = $state(false);
  let memoryHint = $state<string | null>(null);
  let globalAgentScan = $state<GlobalAgentContextScanDto | null>(null);
  let scanningGlobalAgents = $state(false);
  let globalAgentScanError = $state("");

  function sourceDetection(provider: string): AgentSourceDetectionDto | undefined {
    return globalAgentScan?.sources.find((source) => source.provider === provider);
  }

  function arraysEqual(a: string[], b: string[]): boolean {
    return a.length === b.length && a.every((value, index) => value === b[index]);
  }

  function applyDetectedGlobalSources() {
    if (!globalAgentScan) return;
    const nextInstructions = [...globalAgentScan.detectedInstructionSources];
    const nextSkills = [...globalAgentScan.detectedSkillSources];
    if (
      !arraysEqual(runtime.instructionSources, nextInstructions)
      || !arraysEqual(runtime.skillSources, nextSkills)
    ) {
      runtime.instructionSources = nextInstructions;
      runtime.skillSources = nextSkills;
      runtimeChanged = true;
    }
  }

  async function scanGlobalSources(autoSelectEmpty: boolean) {
    if (scanningGlobalAgents) return;
    scanningGlobalAgents = true;
    globalAgentScanError = "";
    try {
      globalAgentScan = await scanGlobalAgentContext();
      if (autoSelectEmpty && globalAgentScan) {
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
      }
    } catch (e) {
      globalAgentScanError = String(e);
    } finally {
      scanningGlobalAgents = false;
    }
  }

  async function refresh() {
    try {
      [proxy, runtime] = await Promise.all([getProxy(), getGlobalRuntimeSettings()]);
      changed = false;
      runtimeChanged = false;
      await scanGlobalSources(true);
    } catch (e) {
      await message(String(e), { title: "加载失败", kind: "error" });
    }
  }

  async function save() {
    saving = true;
    try {
      await setProxy(proxy);
      changed = false;
      await message("代理设置已保存。", { title: "已保存", kind: "info" });
    } catch (e) {
      await message(String(e), { title: "保存失败", kind: "error" });
    } finally {
      saving = false;
    }
  }

  function handleChange() {
    changed = true;
  }

  function handleRuntimeChange() {
    runtimeChanged = true;
  }

  async function saveRuntime() {
    runtimeSaving = true;
    try {
      await setGlobalRuntimeSettings(runtime);
      runtimeChanged = false;
      await message("全局 Runtime 设置已保存。运行状态恢复开关会在下次启动应用时生效；其他 Runtime 配置需要重启对应服务后生效。", { title: "已保存", kind: "info" });
    } catch (e) {
      await message(String(e), { title: "保存失败", kind: "error" });
    } finally {
      runtimeSaving = false;
    }
  }

  async function openLink(url: string, title: string) {
    try {
      await openUrl(url);
    } catch (e) {
      await message(String(e), { title, kind: "error" });
    }
  }

  async function handleCheckUpdate() {
    if (checkingUpdate) return;
    checkingUpdate = true;
    try {
      const result = await checkAppUpdate();
      if (result.updateAvailable) {
        const openRelease = await ask(
          `发现新版本 ${result.latestTag}（当前 v${result.currentVersion}）。是否打开 Releases 页面下载？`,
          { title: "有可用更新", kind: "info", okLabel: "打开下载页", cancelLabel: "稍后" },
        );
        if (openRelease) {
          await openUrl(result.releaseUrl || RELEASES_LATEST_URL);
        }
      } else {
        await message(`当前已是最新版本（v${result.currentVersion}）。`, {
          title: "检查更新",
          kind: "info",
        });
      }
    } catch (e) {
      await message(String(e), { title: "检查更新失败", kind: "error" });
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
    const ok = await ask(
      "将重建界面进程（WebView）以释放内存。MCP 与 FRP 隧道会继续在后台运行，不会被停止。",
      { title: "释放界面内存", kind: "info", okLabel: "立即释放", cancelLabel: "取消" },
    );
    if (!ok) return;
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
    <div class="tx-card p-4">
      <h3 class="text-sm font-semibold">关于</h3>
      <p class="mt-1 text-xs text-[var(--color-text-muted)]">
        当前版本 v{APP_VERSION}。仓库与新版本安装包都在 GitHub Releases。
      </p>
      <div class="mt-4 flex flex-wrap gap-2">
        <Button
          variant="ghost"
          size="md"
          onclick={() => void openLink(REPO_URL, "无法打开仓库")}
        >
          <ExternalLink size={14} strokeWidth={2} />
          <span>打开仓库</span>
        </Button>
        <Button
          variant="ghost"
          size="md"
          onclick={() => void openLink(RELEASES_LATEST_URL, "无法打开 Releases")}
        >
          <ExternalLink size={14} strokeWidth={2} />
          <span>打开 Releases</span>
        </Button>
        <Button
          variant="primary"
          size="md"
          disabled={checkingUpdate}
          busy={checkingUpdate}
          onclick={() => void handleCheckUpdate()}
        >
          <RefreshCw size={14} strokeWidth={2} class={checkingUpdate ? "animate-spin" : ""} />
          <span>{checkingUpdate ? "检查中…" : "检查更新"}</span>
        </Button>
      </div>
    </div>

    <div class="tx-card p-5">
      <h3 class="text-sm font-semibold tracking-tight text-[var(--text-main)]">界面内存</h3>
      <p class="mt-1 text-xs text-[var(--color-text-muted)]">
        长时间运行后 WebView 可能占用较高内存。释放会重建界面进程，不会停止 MCP 或隧道。
      </p>
      {#if memoryHint}
        <p class="mt-2 text-xs text-[var(--color-text-muted)]">{memoryHint}</p>
      {/if}
      <div class="mt-4 flex flex-wrap gap-2">
        <Button
          variant="ghost"
          size="md"
          onclick={() => void refreshMemoryHint()}
        >
          <span>刷新占用</span>
        </Button>
        <Button
          variant="primary"
          size="md"
          disabled={releasingUi}
          busy={releasingUi}
          onclick={() => void handleReleaseUiMemory()}
        >
          <RefreshCw size={14} strokeWidth={2} class={releasingUi ? "animate-spin" : ""} />
          <span>{releasingUi ? "刷新中…" : "释放界面内存"}</span>
        </Button>
      </div>
    </div>

    <div class="tx-card p-5">
      <h3 class="text-sm font-semibold tracking-tight text-[var(--text-main)]">Agent Runtime</h3>
      <p class="mt-1 text-xs text-[var(--color-text-muted)]">
        为所有 Workspace 提供默认可执行 PATH 和 AI Instructions；Workspace 配置会在此基础上覆盖或追加。
      </p>
      <form
        class="mt-4 grid gap-3.5"
        onsubmit={(e) => { e.preventDefault(); void saveRuntime(); }}
      >
        <div class="p-3.5 rounded-xl border border-[var(--border)] bg-[var(--card-bg)]">
          <Toggle
            bind:checked={runtime.restoreRuntimeStateOnLaunch}
            label="启动时恢复上次运行状态"
            description="默认关闭。开启后会记住哪些 Workspace 的 MCP 正在运行，并在下次启动 Coding Tools 时自动恢复。"
            onchange={handleRuntimeChange}
          />
          {#if runtime.restoreRuntimeStateOnLaunch}
            <p class="mt-2 text-xs text-[var(--primary)] pl-0.5">
              首次开启并保存时，会立即记录当前已经运行的服务；之后手动启动或停止都会同步更新恢复状态。
            </p>
          {/if}
        </div>

        <div class="p-3.5 rounded-xl border border-[var(--border)] bg-[var(--card-bg)]">
          <Toggle
            bind:checked={runtime.allowLanAccess}
            label="允许局域网访问"
            description="默认关闭。开启后 MCP 和 Global Gateway 会从仅监听 127.0.0.1 改为监听 0.0.0.0，同一局域网内的服务器即可访问本机端口用于内网穿透。"
            onchange={handleRuntimeChange}
          />
          {#if runtime.allowLanAccess}
            <p class="mt-2 text-xs text-[var(--warning)] pl-0.5">
              已开启：服务会暴露到局域网。建议同时启用认证，并确认防火墙规则符合预期。
            </p>
          {/if}
        </div>

        <label class="grid gap-1">
          <span class="text-xs text-[var(--color-text-muted)]">全局可执行 PATH（每行一个目录，也可粘贴 PATH）</span>
          <textarea
            class="min-h-24 rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-2.5 py-1.5 font-mono text-sm"
            placeholder="/opt/homebrew/bin&#10;/usr/local/bin&#10;~/.local/bin"
            bind:value={runtime.executablePaths}
            oninput={handleRuntimeChange}
          ></textarea>
          <span class="text-xs text-[var(--color-text-muted)]">支持绝对路径和 ~；用于查找 aws、docker、kubectl 等系统程序，仍受 Workspace 的 allowed_commands 策略约束。</span>
        </label>
        <div class="rounded-xl border border-[var(--border)] bg-[var(--surface-main)] p-4 shadow-sm">
          <div class="flex flex-wrap items-start justify-between gap-3">
            <div>
              <p class="text-sm font-semibold text-[var(--text-main)]">全局 Agent Sources</p>
              <p class="mt-1 text-xs text-[var(--color-text-muted)] leading-relaxed">
                自动扫描本机 IDE 的全局 Instructions / Skills；首次没有配置时会自动勾选检测到的来源，之后仍由用户决定是否保存或调整。
              </p>
            </div>
            <div class="flex flex-wrap items-center gap-2">
              <Button
                type="button"
                variant="secondary"
                size="sm"
                disabled={scanningGlobalAgents}
                busy={scanningGlobalAgents}
                onclick={() => void scanGlobalSources(false)}
              >
                {scanningGlobalAgents ? "扫描中…" : "重新扫描"}
              </Button>
              <Button
                type="button"
                variant="ghost"
                size="sm"
                disabled={!globalAgentScan || globalAgentScan.sources.length === 0}
                onclick={applyDetectedGlobalSources}
              >
                应用检测结果
              </Button>
            </div>
          </div>
          {#if globalAgentScanError}
            <p class="mt-2 text-xs text-[var(--danger)]">{globalAgentScanError}</p>
          {:else if globalAgentScan}
            <p class="mt-2 text-xs text-[var(--color-text-muted)]">
              已检测到 {globalAgentScan.sources.length} 个 IDE / Agent 来源 · Instructions {globalAgentScan.detectedInstructionSources.length} 类 · Skills {globalAgentScan.detectedSkillSources.length} 类
            </p>
          {/if}
          <div class="mt-3 grid gap-3 md:grid-cols-2">
            <div>
              <p class="mb-2 text-xs font-medium">Instructions</p>
              <div class="grid gap-2">
                {#each AGENT_SOURCE_OPTIONS as option}
                  {@const detected = sourceDetection(option.value)}
                  <label class="flex items-start gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={runtime.instructionSources.includes(option.value)}
                      onchange={(event) => {
                        runtime.instructionSources = toggleSource(runtime.instructionSources, option.value, event.currentTarget.checked);
                        handleRuntimeChange();
                      }}
                    />
                    <span>
                      <span class="block">{option.label}</span>
                      {#if detected?.instructionPaths.length}
                        <span class="block text-[11px] text-[var(--color-text-muted)]">已检测 {detected.instructionPaths.length} 个全局提示词文件</span>
                      {/if}
                    </span>
                  </label>
                {/each}
              </div>
            </div>
            <div>
              <p class="mb-2 text-xs font-medium">Skills</p>
              <div class="grid gap-2">
                {#each AGENT_SOURCE_OPTIONS as option}
                  {@const detected = sourceDetection(option.value)}
                  <label class="flex items-start gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={runtime.skillSources.includes(option.value)}
                      onchange={(event) => {
                        runtime.skillSources = toggleSource(runtime.skillSources, option.value, event.currentTarget.checked);
                        handleRuntimeChange();
                      }}
                    />
                    <span>
                      <span class="block">{option.label}</span>
                      {#if detected?.skillPaths.length}
                        <span class="block text-[11px] text-[var(--color-text-muted)]">已检测 {detected.skillPaths.length} 个全局 Skill</span>
                      {/if}
                    </span>
                  </label>
                {/each}
              </div>
            </div>
          </div>
          {#if globalAgentScan && globalAgentScan.sources.length > 0}
            <details class="mt-3 text-xs">
              <summary class="cursor-pointer text-[var(--color-text-muted)]">查看全局扫描结果</summary>
              <div class="mt-2 grid gap-3">
                {#each globalAgentScan.sources as source}
                  <div class="rounded-md border border-[var(--color-border)] p-2.5">
                    <p class="font-medium">{AGENT_SOURCE_OPTIONS.find((option) => option.value === source.provider)?.label ?? source.provider}</p>
                    {#each source.instructionPaths as path}
                      <p class="mt-1 font-mono text-[11px] text-[var(--color-text-muted)]">Instruction · {path}</p>
                    {/each}
                    {#each source.skillPaths as path}
                      <p class="mt-1 font-mono text-[11px] text-[var(--color-text-muted)]">Skill · {path}</p>
                    {/each}
                  </div>
                {/each}
              </div>
            </details>
          {/if}
          {#if runtime.instructionSources.includes("custom") || runtime.skillSources.includes("custom")}
            <div class="mt-3 grid gap-3 md:grid-cols-2">
              {#if runtime.instructionSources.includes("custom")}
                <label class="grid gap-1">
                  <span class="text-xs text-[var(--color-text-muted)]">Custom Instructions 文件</span>
                  <textarea class="min-h-20 tx-input tx-mono" bind:value={runtime.customInstructionPaths} oninput={handleRuntimeChange}></textarea>
                </label>
              {/if}
              {#if runtime.skillSources.includes("custom")}
                <label class="grid gap-1">
                  <span class="text-xs text-[var(--color-text-muted)]">Custom Skills 目录</span>
                  <textarea class="min-h-20 tx-input tx-mono" bind:value={runtime.customSkillPaths} oninput={handleRuntimeChange}></textarea>
                </label>
              {/if}
            </div>
          {/if}
        </div>

        <label class="grid gap-1">
          <span class="text-xs text-[var(--color-text-muted)]">全局 AI Instructions</span>
          <textarea
            class="min-h-32 rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-2.5 py-1.5 text-sm"
            placeholder="例如：优先遵循 Clean Code；修改完成后运行相关测试。"
            bind:value={runtime.aiInstructions}
            oninput={handleRuntimeChange}
          ></textarea>
          <span class="text-xs text-[var(--color-text-muted)]">通过 MCP initialize.instructions 注入；Workspace Instructions 会追加在全局规则之后。</span>
        </label>
        <div class="flex justify-end pt-1">
          <Button
            type="submit"
            variant="primary"
            size="md"
            disabled={!runtimeChanged || runtimeSaving}
            busy={runtimeSaving}
          >
            {runtimeSaving ? "保存中…" : "保存 Runtime 设置"}
          </Button>
        </div>
      </form>
    </div>

    <div class="tx-card p-5">
      <h3 class="text-sm font-semibold tracking-tight text-[var(--text-main)]">网络代理</h3>
      <form
        class="mt-4 grid gap-3.5"
        onsubmit={(e) => { e.preventDefault(); void save(); }}
      >
        <div>
          <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1" for="proxy-mode-select">代理模式</label>
          <Select
            options={[
              { value: "none", label: "无代理" },
              { value: "system", label: "系统代理" },
              { value: "manual", label: "手动代理地址" },
            ]}
            bind:value={proxy.mode}
            onchange={handleChange}
          />
        </div>

        {#if proxy.mode === "manual"}
          <div>
            <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1" for="proxy-url-input">代理地址</label>
            <TextInput
              mono
              placeholder="http://127.0.0.1:7890"
              bind:value={proxy.url}
              oninput={handleChange}
            />
            <span class="block text-[11px] text-[var(--color-text-muted)] mt-1">
              支持 HTTP/HTTPS/SOCKS 代理，如 http://127.0.0.1:7890
            </span>
          </div>
        {/if}

        <div class="flex justify-end pt-1">
          <Button
            type="submit"
            variant="primary"
            size="md"
            disabled={!changed || saving}
            busy={saving}
          >
            {saving ? "保存中…" : "保存设置"}
          </Button>
        </div>
      </form>
    </div>
  </div>
</section>
