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
    onAddWorkspace?: () => void | Promise<void>;
    onOpenSettings?: () => void | Promise<void>;
    settingsActive?: boolean;
    settingsNav?: Snippet;
  }

  let {
    children,
    sidebar,
    onAddWorkspace,
    onOpenSettings,
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
    <div class="tx-sidebar-header">
      <div class="flex items-start justify-between gap-2">
        <div class="tx-brand-lockup">
          <div class="tx-brand-mark" aria-hidden="true">CT</div>
          <div class="min-w-0">
            <p class="tx-brand-kicker">Coding Tools</p>
            <h1 class="tx-brand-title">桌面控制台</h1>
          </div>
        </div>
        <ThemeToggle />
      </div>
      {#if onAddWorkspace}
        <button type="button" class="tx-btn-primary tx-btn-sidebar" onclick={onAddWorkspace}>
          <Plus size={15} strokeWidth={2.2} />
          添加工作区
        </button>
      {/if}
    </div>

    <div class="tx-sidebar-body">
      {#if onAddWorkspace}
        <p class="tx-sidebar-section-label">工作区</p>
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
          <Settings size={16} strokeWidth={2} />
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
      <div class="tx-settings-tabs">
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
