<script lang="ts">
  import type { Snippet } from "svelte";
  import Modal from "./Modal.svelte";
  import Button from "./Button.svelte";
  import { AlertTriangle, AlertCircle, Info } from "@lucide/svelte";

  interface Props {
    open: boolean;
    title: string;
    message?: string;
    detail?: string;
    confirmText?: string;
    cancelText?: string;
    severity?: "danger" | "warning" | "info";
    busy?: boolean;
    onConfirm: () => void | Promise<void>;
    onCancel: () => void;
    children?: Snippet;
  }

  let {
    open,
    title,
    message,
    detail,
    confirmText = "确认",
    cancelText = "取消",
    severity = "danger",
    busy = false,
    onConfirm,
    onCancel,
    children,
  }: Props = $props();

  const iconConfig = {
    danger: {
      icon: AlertTriangle,
      bg: "bg-[var(--danger-soft)] text-[var(--danger)] border border-[var(--danger)]/20",
    },
    warning: {
      icon: AlertCircle,
      bg: "bg-[var(--warning-soft)] text-[var(--warning)] border border-[var(--warning)]/20",
    },
    info: {
      icon: Info,
      bg: "bg-[var(--primary-soft)] text-[var(--primary)] border border-[var(--primary)]/20",
    },
  };
</script>

<Modal {open} onclose={onCancel} showClose={!busy} maxWidth="max-w-md">
  {@const currentConfig = iconConfig[severity]}
  {@const Icon = currentConfig.icon}
  <div class="flex items-start gap-3.5">
    <div class="size-10 rounded-xl flex items-center justify-center shrink-0 {currentConfig.bg}">
      <Icon size={20} strokeWidth={2.2} />
    </div>
    <div class="min-w-0 flex-1">
      <h3 class="text-base font-semibold text-[var(--text-main)] leading-snug tracking-tight">
        {title}
      </h3>
      {#if message}
        <p class="text-xs text-[var(--text-secondary)] mt-1.5 leading-relaxed">
          {message}
        </p>
      {/if}
      {#if children}
        <div class="text-xs text-[var(--text-secondary)] mt-1.5 leading-relaxed">
          {@render children()}
        </div>
      {/if}
      {#if detail}
        <div class="mt-2.5 p-2 rounded-lg bg-[var(--surface-hover)] border border-[var(--border)] text-xs text-[var(--text-muted)] font-mono break-all">
          {detail}
        </div>
      {/if}
    </div>
  </div>

  <div class="mt-6 flex items-center justify-end gap-2.5 pt-3 border-t border-[var(--border)]">
    <Button
      variant="ghost"
      size="md"
      disabled={busy}
      onclick={onCancel}
    >
      {cancelText}
    </Button>
    <Button
      variant={severity === "danger" ? "danger" : "primary"}
      size="md"
      {busy}
      onclick={onConfirm}
    >
      {confirmText}
    </Button>
  </div>
</Modal>
