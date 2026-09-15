<script lang="ts">
  import "../app.css";
  import "../styles/workbench.css";
  import { onMount } from "svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { open } from "@tauri-apps/plugin-dialog";
  import AppShell from "$lib/components/AppShell.svelte";
  import ToastHost from "$lib/components/ToastHost.svelte";
  import WorkspaceNavItem from "$lib/components/WorkspaceNavItem.svelte";
  import {
    createWorkspace,
    getRuntimeStatus,
    listWorkspaces,
    restoreRuntimeState,
  } from "$lib/api/workspaces";
  import { mcpRuntimeStates, workspaces } from "$lib/stores/app";
  import { showToast } from "$lib/stores/toast";
  import { startUiMemoryGuard } from "$lib/ui-memory-guard";
  import { startCloseGuard } from "$lib/close-guard";
  import CloseConfirmDialog from "$lib/components/CloseConfirmDialog.svelte";
  import type { RuntimeState } from "$lib/types";
  import {
    dashboardPreferences,
    loadDashboardPreferences,
    sortWorkspaceIds,
  } from "$lib/dashboard-preferences";

  let { children } = $props();
  let closeConfirmOpen = $state(false);
  const sidebarWorkspaces = $derived.by(() => {
    const orderedIds = sortWorkspaceIds(
      $workspaces.map((workspace) => workspace.id),
      $dashboardPreferences,
    );
    const byId = new Map($workspaces.map((workspace) => [workspace.id, workspace]));
    return orderedIds.map((id) => byId.get(id)).filter(Boolean);
  });

  async function refreshWorkspaces() {
    const items = await listWorkspaces();
    workspaces.set(items);

    const mcpStates: Record<string, RuntimeState> = {};
    await Promise.all(
      items.map(async (item) => {
        try {
          const mcp = await getRuntimeStatus(item.id);
          mcpStates[item.id] = mcp.state;
        } catch {
          mcpStates[item.id] = "stopped";
        }
      }),
    );
    mcpRuntimeStates.set(mcpStates);
  }

  async function addWorkspace() {
    try {
      const selected = await open({ directory: true, multiple: false });
      if (!selected || Array.isArray(selected)) return;
      const profile = await createWorkspace(selected);
      await refreshWorkspaces();
      goto(`/workspace/${profile.id}`);
    } catch (error) {
      showToast(String(error), {
        title: "添加工作区失败",
        kind: "error",
        duration: 8000,
      });
    }
  }

  function openWorkspace(id: string) {
    if ($page.url.pathname === `/workspace/${id}`) {
      goto("/");
      return;
    }
    goto(`/workspace/${id}`);
  }

  function openGeneralSettings() {
    goto("/settings/general");
  }

  function openKeysSettings() {
    goto("/settings/keys");
  }

  function openConnectionSettings() {
    goto("/settings/gateway");
  }

  onMount(() => {
    loadDashboardPreferences();
    const stopGuard = startUiMemoryGuard();
    const stopClose = startCloseGuard(() => {
      closeConfirmOpen = true;
    });
    void (async () => {
      try {
        await restoreRuntimeState();
      } catch (error) {
        showToast(String(error), {
          title: "恢复上次运行状态失败",
          kind: "warning",
          duration: 8000,
        });
      }
      await refreshWorkspaces();
    })();
    const handleAddWorkspace = () => void addWorkspace();
    window.addEventListener("coding-tools:add-workspace", handleAddWorkspace);
    return () => {
      stopGuard();
      stopClose();
      window.removeEventListener("coding-tools:add-workspace", handleAddWorkspace);
    };
  });
  import SegmentedControl from "$lib/components/ui/SegmentedControl.svelte";

  const settingsNavItems = [
    { value: "general", label: "通用" },
    { value: "keys", label: "密钥与认证" },
    { value: "connection", label: "连接" },
  ];

  const activeSettingsNav = $derived(
    $page.url.pathname === "/settings/keys"
      ? "keys"
      : ["/settings/gateway", "/settings/frp", "/settings/software"].includes($page.url.pathname)
        ? "connection"
        : "general",
  );

  function handleSettingsNavChange(value: string) {
    if (value === "keys") openKeysSettings();
    else if (value === "connection") openConnectionSettings();
    else openGeneralSettings();
  }
</script>

<AppShell
  onOpenDashboard={() => goto("/")}
  onAddWorkspace={addWorkspace}
  onOpenSettings={openGeneralSettings}
  dashboardActive={$page.url.pathname === "/"}
  settingsActive={$page.url.pathname.startsWith("/settings")}
>
  {#snippet settingsNav()}
    <SegmentedControl
      items={settingsNavItems}
      value={activeSettingsNav}
      onchange={handleSettingsNavChange}
      size="md"
    />
  {/snippet}
  {#snippet sidebar()}
    <div class="wb-sidebar-workspaces">
      {#each sidebarWorkspaces as workspace (workspace!.id)}
        <WorkspaceNavItem
          workspace={workspace!}
          active={$page.url.pathname === `/workspace/${workspace!.id}`}
          mcpState={$mcpRuntimeStates[workspace!.id] ?? "stopped"}
          onClick={() => openWorkspace(workspace!.id)}
        />
      {/each}
    </div>
  {/snippet}

  {#snippet children()}
    {#key $page.url.pathname}
      <div class="tx-page-transition">
        {@render children()}
      </div>
    {/key}
  {/snippet}
</AppShell>

<ToastHost />
<CloseConfirmDialog
  open={closeConfirmOpen}
  onCancel={() => {
    closeConfirmOpen = false;
  }}
/>
