<script lang="ts">
  import ThemeToggle from "$lib/components/ThemeToggle.svelte";
  import { APP_VERSION } from "$lib/app-version";
  import { REPO_URL } from "$lib/app-links";
  import { openUrl } from "$lib/api/app-info";
  import { message } from "@tauri-apps/plugin-dialog";
  import { Github, Plus, Settings } from "@lucide/svelte";
  import type { Snippet } from "svelte";

  interface Props {
    children: Snippet;
    sidebar: Snippet;
    onOpenDashboard?: () => void | Promise<void>;
    onAddWorkspace?: () => void | Promise<void>;
    onOpenSettings?: () => void | Promise<void>;
    dashboardActive?: boolean;
    settingsActive?: boolean;
    settingsNav?: Snippet;
  }

  let {
    children,
    sidebar,
    onOpenDashboard,
    onAddWorkspace,
    onOpenSettings,
    dashboardActive = false,
    settingsActive = false,
    settingsNav,
  }: Props = $props();

  async function openRepo() {
    try {
      await openUrl(REPO_URL);
    } catch (e) {
      await message(String(e), { title: "无法打开仓库", kind: "error" });
    }
  }
</script>

<div class="app-layout">
  <aside class="tx-sidebar">
    <div class="tx-sidebar-header" data-tauri-drag-region>
      <div class="flex items-start justify-between gap-2" data-tauri-drag-region>
        <button
          type="button"
          class="wb-brand-card"
          class:active={dashboardActive}
          onclick={onOpenDashboard}
          title="返回工作台"
        >
          <div class="tx-brand-mark" aria-hidden="true">CT</div>
          <div class="min-w-0 text-left">
            <p class="tx-brand-kicker">Coding Tools</p>
            <h1 class="tx-brand-title">桌面控制台</h1>
          </div>
        </button>
        <ThemeToggle />
      </div>
    </div>

    <div class="tx-sidebar-body">
      {#if onAddWorkspace}
        <div class="wb-sidebar-section-head">
          <p class="tx-sidebar-section-label">工作区</p>
          <button type="button" class="wb-sidebar-add" onclick={onAddWorkspace} title="添加工作区">
            <Plus size={13} strokeWidth={2.2} />
          </button>
        </div>
      {/if}
      {@render sidebar()}
    </div>

    <div class="tx-sidebar-footer">
      {#if onOpenSettings}
        <button
          type="button"
          class="tx-sidebar-settings"
          class:active={settingsActive}
          onclick={onOpenSettings}
        >
          <Settings size={15} strokeWidth={2} />
          <span>设置</span>
        </button>
      {/if}
      <div class="tx-app-meta">
        <p class="tx-app-version">v{APP_VERSION}</p>
        <button type="button" class="tx-repo-link" onclick={() => void openRepo()}>
          <Github size={12} strokeWidth={2} />
          <span>仓库</span>
        </button>
      </div>
    </div>
  </aside>

  <main class="tx-main">
    {#if settingsActive && settingsNav}
      <div class="tx-settings-tabs" data-tauri-drag-region>
        <div class="tx-settings-tabs-inner">
          {@render settingsNav()}
        </div>
      </div>
    {/if}
    {@render children()}
  </main>
</div>

<svelte:head>
  <title>Coding Tools MCP</title>
</svelte:head>
