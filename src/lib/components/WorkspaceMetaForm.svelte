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
  class="flex flex-col gap-5"
  onsubmit={(event) => {
    event.preventDefault();
    void save();
  }}
>
  <!-- Row 1: Workspace Name -->
  <div>
    <span class="block text-xs font-medium text-[var(--text-secondary)] mb-1.5">
      工作区名称
    </span>
    <div class="flex items-center gap-3">
      <div class="min-w-0 flex-1">
        <TextInput
          bind:value={draftName}
          placeholder="输入工作区名称"
        />
      </div>
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
  </div>

  <!-- Row 2: Physical Path -->
  <div>
    <span class="block text-xs font-medium text-[var(--text-secondary)] mb-1.5">物理路径</span>
    <div class="flex flex-col gap-2 sm:flex-row sm:items-center">
      <p
        class="font-mono text-xs min-w-0 flex-1 truncate rounded-lg border border-[var(--border)] bg-[var(--surface-hover)] px-3 py-2 text-[var(--text-secondary)] select-all"
        title={path}
      >
        {path}
      </p>
      <div class="flex items-center gap-2 shrink-0">
        <Button
          variant="secondary"
          size="sm"
          disabled={opening || !path.trim()}
          busy={opening}
          onclick={() => void openDirectory()}
        >
          <FolderOpen size={13} strokeWidth={2} />
          <span>打开目录</span>
        </Button>
        <Button
          variant="secondary"
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
  </div>
</form>
