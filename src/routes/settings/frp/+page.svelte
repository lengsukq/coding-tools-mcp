<script lang="ts">
  import { onMount } from "svelte";
  import ConnectionSettingsNav from "$lib/components/ConnectionSettingsNav.svelte";
  import { message } from "@tauri-apps/plugin-dialog";
  import {
    deleteFrpProfile,
    listFrpProfiles,
    saveFrpProfile,
    type FrpProfileDto,
  } from "$lib/api/settings";
  import SecretInput from "$lib/components/SecretInput.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";
  import ConfirmDialog from "$lib/components/ui/ConfirmDialog.svelte";

  let profiles = $state<FrpProfileDto[]>([]);
  let loading = $state(true);
  let saving = $state(false);
  let editingId = $state<string | null>(null);
  let name = $state("");
  let server = $state("");
  let serverPort = $state(7000);
  let token = $state("");

  let deleteConfirmOpen = $state(false);
  let pendingDeleteProfile = $state<FrpProfileDto | null>(null);
  let deleteBusy = $state(false);

  async function refresh() {
    loading = true;
    try {
      profiles = await listFrpProfiles();
    } finally {
      loading = false;
    }
  }

  function resetForm() {
    editingId = null;
    name = "";
    server = "";
    serverPort = 7000;
    token = "";
  }

  function editProfile(profile: FrpProfileDto) {
    editingId = profile.id;
    name = profile.name;
    server = profile.server;
    serverPort = profile.serverPort;
    token = "";
  }

  async function save() {
    if (!name.trim() || !server.trim()) {
      await message("请填写配置名称和服务器地址。", { title: "无法保存", kind: "warning" });
      return;
    }
    saving = true;
    try {
      await saveFrpProfile(
        {
          id: editingId ?? "",
          name: name.trim(),
          server: server.trim(),
          serverPort,
        },
        token.trim() || undefined,
      );
      resetForm();
      await refresh();
    } catch (error) {
      await message(String(error), { title: "保存失败", kind: "error" });
    } finally {
      saving = false;
    }
  }

  function requestRemoveProfile(profile: FrpProfileDto) {
    pendingDeleteProfile = profile;
    deleteConfirmOpen = true;
  }

  async function handleConfirmDelete() {
    if (!pendingDeleteProfile || deleteBusy) return;
    deleteBusy = true;
    try {
      await deleteFrpProfile(pendingDeleteProfile.id);
      if (editingId === pendingDeleteProfile.id) {
        resetForm();
      }
      deleteConfirmOpen = false;
      pendingDeleteProfile = null;
      await refresh();
    } catch (error) {
      await message(String(error), { title: "删除失败", kind: "error" });
    } finally {
      deleteBusy = false;
    }
  }

  onMount(refresh);
</script>

<section class="page-scroll">
  <header class="page-header">
    <p class="page-kicker">全局设置</p>
    <h2 class="page-title">FRP 配置</h2>
    <p class="mt-2 max-w-2xl text-xs leading-relaxed text-[var(--color-text-muted)]">
      在此配置 FRP 服务器、端口与 Token。各工作区只需选择配置并填写自己的子域名；修改子域名后保存会自动更新
      frpc 配置并重启公网隧道。
    </p>
  </header>

  <ConnectionSettingsNav />

  <div class="page-body flex flex-col gap-6">
    <div class="tx-card p-5">
      <h3 class="text-sm font-semibold tracking-tight text-[var(--text-main)]">
        {editingId ? "编辑配置" : "新建配置"}
      </h3>
      <form
        class="mt-4 grid gap-3.5"
        onsubmit={(event) => {
          event.preventDefault();
          void save();
        }}
      >
        <div>
          <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1" for="frp-name-input">名称</label>
          <TextInput
            placeholder="公司 FRP / 家庭内网穿透"
            bind:value={name}
          />
        </div>
        <div>
          <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1" for="frp-server-input">服务器域名</label>
          <TextInput
            mono
            placeholder="frp.example.com"
            bind:value={server}
          />
        </div>
        <div>
          <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1" for="frp-port-input">端口</label>
          <input
            id="frp-port-input"
            type="number"
            min="1"
            max="65535"
            class="w-full px-3 py-1.5 text-xs font-mono rounded-lg border border-[var(--border)] bg-[var(--card-bg)] text-[var(--text-main)] focus:outline-none focus:ring-2 focus:ring-[var(--primary)]/25 focus:border-[var(--primary)] transition-all"
            bind:value={serverPort}
          />
        </div>
        <div>
          <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1" for="frp-token-input">
            Token {editingId ? "（留空则保持不变）" : ""}
          </label>
          <SecretInput
            bind:value={token}
            placeholder="frp auth token"
            showCopy={false}
          />
        </div>
        <div class="flex items-center gap-2 pt-1">
          <Button
            type="submit"
            variant="primary"
            size="md"
            disabled={saving}
            busy={saving}
          >
            {saving ? "保存中…" : editingId ? "更新配置" : "添加配置"}
          </Button>
          {#if editingId}
            <Button
              type="button"
              variant="ghost"
              size="md"
              onclick={resetForm}
            >
              取消
            </Button>
          {/if}
        </div>
      </form>
    </div>

    <div class="tx-card p-5">
      <h3 class="text-sm font-semibold tracking-tight text-[var(--text-main)]">已保存的配置</h3>
      {#if loading}
        <p class="mt-4 text-xs text-[var(--color-text-muted)]">加载中…</p>
      {:else if profiles.length === 0}
        <p class="mt-4 text-xs text-[var(--color-text-muted)]">暂无 FRP 配置。</p>
      {:else}
        <ul class="mt-4 space-y-2.5">
          {#each profiles as profile (profile.id)}
            <li
              class="flex items-center justify-between gap-3 p-3 rounded-xl border border-[var(--border)] bg-[var(--surface-hover)] transition-all hover:border-[var(--primary)]/30"
            >
              <div class="min-w-0">
                <p class="truncate text-xs font-semibold text-[var(--text-main)]">{profile.name}</p>
                <p class="truncate font-mono text-[11px] text-[var(--color-text-muted)] mt-0.5">
                  {profile.server}:{profile.serverPort}
                  · Token {profile.hasToken ? "已配置" : "未配置"}
                </p>
              </div>
              <div class="flex shrink-0 items-center gap-1.5">
                <Button
                  variant="ghost"
                  size="sm"
                  onclick={() => editProfile(profile)}
                >
                  编辑
                </Button>
                <Button
                  variant="danger"
                  size="sm"
                  onclick={() => requestRemoveProfile(profile)}
                >
                  删除
                </Button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
  </div>

  <ConfirmDialog
    open={deleteConfirmOpen}
    title="删除 FRP 配置"
    message={`确定删除配置「${pendingDeleteProfile?.name}」？已使用此配置的所有工作区将无法通过该服务器建立 FRP 公网隧道。`}
    detail={pendingDeleteProfile ? `${pendingDeleteProfile.server}:${pendingDeleteProfile.serverPort}` : undefined}
    confirmText="确认删除"
    cancelText="取消"
    severity="danger"
    busy={deleteBusy}
    onConfirm={handleConfirmDelete}
    onCancel={() => {
      deleteConfirmOpen = false;
      pendingDeleteProfile = null;
    }}
  />
</section>
