<script lang="ts">
  import Modal from "$lib/components/ui/Modal.svelte";
  import Button from "$lib/components/ui/Button.svelte";
  import { hideToTray, quitApp } from "$lib/api/window-chrome";
  import { AlertCircle } from "@lucide/svelte";

  interface Props {
    open?: boolean;
    onCancel?: () => void;
  }

  let { open = false, onCancel }: Props = $props();

  let busy = $state(false);

  async function runBackground() {
    if (busy) return;
    busy = true;
    try {
      await hideToTray();
      onCancel?.();
    } catch (error) {
      console.error("[close-confirm] hide_to_tray failed", error);
    } finally {
      busy = false;
    }
  }

  async function runQuit() {
    if (busy) return;
    busy = true;
    try {
      await quitApp();
    } catch (error) {
      console.error("[close-confirm] quit_app failed", error);
      busy = false;
    }
  }
</script>

<Modal {open} onclose={onCancel} showClose={!busy} maxWidth="max-w-md">
  <div class="flex items-start gap-3.5">
    <div class="size-10 rounded-xl flex items-center justify-center shrink-0 bg-[var(--primary-soft)] text-[var(--primary)] border border-[var(--primary)]/20">
      <AlertCircle size={20} strokeWidth={2.2} />
    </div>
    <div class="min-w-0 flex-1">
      <h3 class="text-base font-semibold text-[var(--text-main)] tracking-tight">
        关闭 Coding Tools MCP?
      </h3>
      <p class="text-xs text-[var(--text-secondary)] mt-1.5 leading-relaxed">
        选择后台运行可隐藏窗口，保持正在运行的 MCP 与公网隧道服务持续可用；后续可通过系统托盘重新呼出。
      </p>
    </div>
  </div>

  <div class="mt-6 flex items-center justify-end gap-2.5 pt-3 border-t border-[var(--border)]">
    <Button
      variant="ghost"
      size="md"
      disabled={busy}
      onclick={() => onCancel?.()}
    >
      取消
    </Button>
    <Button
      variant="secondary"
      size="md"
      disabled={busy}
      onclick={() => void runBackground()}
    >
      后台运行
    </Button>
    <Button
      variant="danger"
      size="md"
      {busy}
      onclick={() => void runQuit()}
    >
      彻底退出
    </Button>
  </div>
</Modal>
