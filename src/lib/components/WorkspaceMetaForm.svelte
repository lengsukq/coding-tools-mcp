<script lang="ts">
  import { FolderInput, FolderOpen } from "@lucide/svelte";
  import { open } from "@tauri-apps/plugin-dialog";
  import { openWorkspaceDirectory } from "$lib/api/workspaces";
  import { showToast } from "$lib/stores/toast";
  import Button from "$lib/components/ui/Button.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";

  interface Props {
    name: string;
    path: string;
    onSave: (name: string) => void | Promise<void>;
    onUpdatePath: (path: string) => void | Promise<void>;
  }

  let { name, path, onSave, onUpdatePath }: Props = $props();

  let draftName = $state("");
  let saving = $state(false);
  let opening = $state(false);
  let updatingPath = $state(false);

  const dirty = $derived(draftName.trim() !== name && draftName.trim().length > 0);

  $effect(() => {
    draftName = name;
  });

  async function save() {
    if (saving || !dirty) return;
    saving = true;
    try {
      await onSave(draftName.trim());
    } finally {
      saving = false;
    }
  }

  async function openDirectory() {
    if (opening || !path.trim()) return;
    opening = true;
    try {
      await openWorkspaceDirectory(path);
    } catch (error) {
      showToast(String(error), {
        kind: "error",
        title: "无法打开目录",
      });
    } finally {
      opening = false;
    }
  }

  function normalizePath(value: string): string {
    return value.trim().replace(/[\\/]+$/, "");
  }

  async function updateDirectory() {
    if (updatingPath) return;
    updatingPath = true;
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        defaultPath: path.trim() || undefined,
      });
      if (!selected || Array.isArray(selected)) return;
      const nextPath = normalizePath(selected);
      if (!nextPath || nextPath === normalizePath(path)) return;
      await onUpdatePath(nextPath);
    } catch (error) {
      showToast(String(error), {
        kind: "error",
        title: "无法更新目录",
      });
    } finally {
      updatingPath = false;
    }
  }
</script>

<form
  class="flex flex-col gap-3.5 sm:flex-row sm:items-end"
  onsubmit={(event) => {
    event.preventDefault();
    void save();
  }}
>
  <div class="min-w-0 flex-1">
    <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1.5" for="workspace-name-input">
      工作区名称
    </label>
    <TextInput
      bind:value={draftName}
      placeholder="输入工作区名称"
    />
  </div>

  <div class="min-w-0 flex-1">
    <span class="block text-xs font-medium text-[var(--text-secondary)] mb-1.5">物理路径</span>
    <div class="flex min-w-0 items-center gap-2">
      <p
        class="font-mono text-xs min-w-0 flex-1 truncate rounded-lg border border-[var(--border)] bg-[var(--surface-hover)] px-2.5 py-1.5 text-[var(--text-secondary)]"
        title={path}
      >
        {path}
      </p>
      <Button
        variant="ghost"
        size="sm"
        disabled={opening || !path.trim()}
        busy={opening}
        onclick={() => void openDirectory()}
      >
        <FolderOpen size={13} strokeWidth={2} />
        <span>打开</span>
      </Button>
      <Button
        variant="ghost"
        size="sm"
        disabled={updatingPath}
        busy={updatingPath}
        onclick={() => void updateDirectory()}
      >
        <FolderInput size={13} strokeWidth={2} />
        <span>更改目录</span>
      </Button>
    </div>
  </div>

  <div class="shrink-0">
    <Button
      type="submit"
      variant="primary"
      size="md"
      disabled={saving || !dirty}
      busy={saving}
    >
      保存名称
    </Button>
  </div>
</form>
