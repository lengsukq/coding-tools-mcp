<script lang="ts">
  import { ExternalLink, RefreshCw } from "@lucide/svelte";
  import Button from "$lib/components/ui/Button.svelte";

  interface Props {
    version: string;
    checkingUpdate: boolean;
    onOpenRepo: () => void | Promise<void>;
    onOpenReleases: () => void | Promise<void>;
    onCheckUpdate: () => void | Promise<void>;
  }

  let { version, checkingUpdate, onOpenRepo, onOpenReleases, onCheckUpdate }: Props = $props();
</script>

<div class="tx-card p-4">
  <h3 class="text-sm font-semibold">关于</h3>
  <p class="mt-1 text-xs text-[var(--color-text-muted)]">
    当前版本 v{version}。仓库与新版本安装包都在 GitHub Releases。
  </p>
  <div class="mt-4 flex flex-wrap gap-2">
    <Button variant="ghost" size="md" onclick={onOpenRepo}>
      <ExternalLink size={14} strokeWidth={2} />
      <span>打开仓库</span>
    </Button>
    <Button variant="ghost" size="md" onclick={onOpenReleases}>
      <ExternalLink size={14} strokeWidth={2} />
      <span>打开 Releases</span>
    </Button>
    <Button
      variant="primary"
      size="md"
      disabled={checkingUpdate}
      busy={checkingUpdate}
      onclick={onCheckUpdate}
    >
      <RefreshCw size={14} strokeWidth={2} class={checkingUpdate ? "animate-spin" : ""} />
      <span>{checkingUpdate ? "检查中…" : "检查更新"}</span>
    </Button>
  </div>
</div>
