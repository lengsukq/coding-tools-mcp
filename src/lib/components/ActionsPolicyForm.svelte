<script lang="ts">
  import type { ActionsConfig } from "$lib/types";
  import Button from "$lib/components/ui/Button.svelte";
  import Select from "$lib/components/ui/Select.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";

  export interface ActionsPolicyDraft {
    allowedCommands: string;
    maxPatchBytes: number;
    permissionMode: string;
  }

  interface Props {
    allowedCommands: string;
    maxPatchBytes: number;
    permissionMode: string;
    onSave: (draft: ActionsPolicyDraft) => void | Promise<void>;
  }

  const PERMISSION_MODE_OPTIONS = [
    { value: "trusted", label: "受信任" },
    { value: "safe", label: "安全受限" },
    { value: "dangerous", label: "完全放开" },
  ] as const;

  let { allowedCommands, maxPatchBytes, permissionMode, onSave }: Props = $props();

  let draftCommands = $state("");
  let draftMaxPatch = $state(200_000);
  let draftMode = $state("trusted");
  let saving = $state(false);

  const dirty = $derived(
    draftCommands !== allowedCommands ||
      draftMaxPatch !== maxPatchBytes ||
      draftMode !== permissionMode,
  );

  $effect(() => {
    draftCommands = allowedCommands;
    draftMaxPatch = maxPatchBytes;
    draftMode = permissionMode;
  });

  async function save() {
    if (saving || !dirty) return;
    saving = true;
    try {
      await onSave({
        allowedCommands: draftCommands.trim(),
        maxPatchBytes: draftMaxPatch,
        permissionMode: draftMode,
      });
    } finally {
      saving = false;
    }
  }
</script>

<form
  class="grid gap-3.5"
  onsubmit={(event) => {
    event.preventDefault();
    void save();
  }}
>
  <div class="grid gap-1.5">
    <span class="text-xs font-medium text-[var(--color-text-muted)]">允许命令（逗号分隔）</span>
    <TextInput
      mono
      placeholder="pytest,python,cargo,npm,..."
      bind:value={draftCommands}
    />
  </div>
  <div class="grid gap-1.5">
    <span class="text-xs font-medium text-[var(--color-text-muted)]">最大 Patch 字节数</span>
    <input
      type="number"
      min="1024"
      max="5000000"
      class="w-full px-3 py-1.5 text-xs rounded-lg border border-[var(--border)] bg-[var(--card-bg)] text-[var(--text-main)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--primary)]/25 focus:border-[var(--primary)] transition-all"
      bind:value={draftMaxPatch}
    />
  </div>
  <label class="grid gap-1.5">
    <span class="text-xs font-medium text-[var(--color-text-muted)]">权限模式</span>
    <Select
      options={PERMISSION_MODE_OPTIONS}
      bind:value={draftMode}
    />
  </label>
  <p class="text-xs text-[var(--color-text-muted)]">
    作用于 Actions gateway 的 exec_command 白名单与 apply_patch 大小限制。
  </p>
  <div class="flex justify-end pt-1">
    <Button
      type="submit"
      variant="primary"
      busy={saving}
      disabled={saving || !dirty}
    >
      {saving ? "保存中…" : "保存策略"}
    </Button>
  </div>
</form>
