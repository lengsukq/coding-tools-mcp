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

  let proxy = $state<ProxyConfigDto>({ mode: "none", url: "" });
  let runtime = $state<GlobalRuntimeSettingsDto>({ executablePaths: "", aiInstructions: "" });
  let changed = $state(false);
  let runtimeChanged = $state(false);
  let saving = $state(false);
  let runtimeSaving = $state(false);
  let checkingUpdate = $state(false);
  let releasingUi = $state(false);
  let memoryHint = $state<string | null>(null);

  async function refresh() {
    try {
      [proxy, runtime] = await Promise.all([getProxy(), getGlobalRuntimeSettings()]);
      changed = false;
      runtimeChanged = false;
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
      await message("全局 Runtime 设置已保存。已运行的 MCP 服务需要重启后生效。", { title: "已保存", kind: "info" });
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
      "将重建界面进程（WebView）以释放内存。MCP、Actions 与 FRP 隧道会继续在后台运行，不会被停止。",
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
        <button
          type="button"
          class="inline-flex items-center gap-1.5 rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-3 py-1.5 text-sm"
          onclick={() => void openLink(REPO_URL, "无法打开仓库")}
        >
          <ExternalLink size={14} strokeWidth={2} />
          打开仓库
        </button>
        <button
          type="button"
          class="inline-flex items-center gap-1.5 rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-3 py-1.5 text-sm"
          onclick={() => void openLink(RELEASES_LATEST_URL, "无法打开 Releases")}
        >
          <ExternalLink size={14} strokeWidth={2} />
          打开 Releases
        </button>
        <button
          type="button"
          class="inline-flex items-center gap-1.5 rounded-md bg-[var(--color-accent)] px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
          disabled={checkingUpdate}
          onclick={() => void handleCheckUpdate()}
        >
          <RefreshCw size={14} strokeWidth={2} class={checkingUpdate ? "animate-spin" : ""} />
          {checkingUpdate ? "检查中…" : "检查更新"}
        </button>
      </div>
    </div>

    <div class="tx-card p-4">
      <h3 class="text-sm font-semibold">界面内存</h3>
      <p class="mt-1 text-xs text-[var(--color-text-muted)]">
        长时间运行后 WebView 可能占用较高内存。释放会重建界面进程，不会停止 MCP 或隧道。
      </p>
      {#if memoryHint}
        <p class="mt-2 text-xs text-[var(--color-text-muted)]">{memoryHint}</p>
      {/if}
      <div class="mt-4 flex flex-wrap gap-2">
        <button
          type="button"
          class="inline-flex items-center gap-1.5 rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-3 py-1.5 text-sm"
          onclick={() => void refreshMemoryHint()}
        >
          刷新占用
        </button>
        <button
          type="button"
          class="inline-flex items-center gap-1.5 rounded-md bg-[var(--color-accent)] px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
          disabled={releasingUi}
          onclick={() => void handleReleaseUiMemory()}
        >
          <RefreshCw size={14} strokeWidth={2} class={releasingUi ? "animate-spin" : ""} />
          {releasingUi ? "刷新中…" : "释放界面内存"}
        </button>
      </div>
    </div>

    <div class="tx-card p-4">
      <h3 class="text-sm font-semibold">Agent Runtime</h3>
      <p class="mt-1 text-xs text-[var(--color-text-muted)]">
        为所有 Workspace 提供默认可执行 PATH 和 AI Instructions；Workspace 配置会在此基础上覆盖或追加。
      </p>
      <form
        class="mt-4 grid gap-3"
        onsubmit={(e) => { e.preventDefault(); void saveRuntime(); }}
      >
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
          <button
            type="submit"
            class="rounded-md bg-[var(--color-accent)] px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
            disabled={!runtimeChanged || runtimeSaving}
          >
            {runtimeSaving ? "保存中…" : "保存 Runtime 设置"}
          </button>
        </div>
      </form>
    </div>

    <div class="tx-card p-4">
      <h3 class="text-sm font-semibold">网络代理</h3>
      <form
        class="mt-4 grid gap-3"
        onsubmit={(e) => { e.preventDefault(); void save(); }}
      >
        <label class="grid gap-1">
          <span class="text-xs text-[var(--color-text-muted)]">代理模式</span>
          <select
            class="rounded-md border border-[var(--color-border)] bg-[var(--color-bg)] px-2.5 py-1.5 text-sm"
            bind:value={proxy.mode}
            onchange={handleChange}
          >
            <option value="none">无代理</option>
            <option value="system">系统代理</option>
            <option value="manual">手动代理地址</option>
          </select>
        </label>

        {#if proxy.mode === "manual"}
          <label class="grid gap-1">
            <span class="text-xs text-[var(--color-text-muted)]">代理地址</span>
            <input
              type="text"
              class="tx-input tx-mono"
              placeholder="http://127.0.0.1:7890"
              bind:value={proxy.url}
              oninput={handleChange}
            />
            <span class="text-xs text-[var(--color-text-muted)]">
              支持 HTTP/HTTPS/SOCKS 代理，如 http://127.0.0.1:7890
            </span>
          </label>
        {/if}

        <div class="flex justify-end pt-1">
          <button
            type="submit"
            class="rounded-md bg-[var(--color-accent)] px-3 py-1.5 text-sm font-medium text-white disabled:opacity-50"
            disabled={!changed || saving}
          >
            {saving ? "保存中…" : "保存设置"}
          </button>
        </div>
      </form>
    </div>
  </div>
</section>
