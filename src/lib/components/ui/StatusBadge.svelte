<script lang="ts">
  interface Props {
    status?: "running" | "starting" | "stopping" | "stopped" | "error" | "success" | "warning" | "info" | "neutral";
    text?: string;
    showDot?: boolean;
    size?: "sm" | "md";
    class?: string;
  }

  let {
    status = "neutral",
    text,
    showDot = true,
    size = "md",
    class: customClass = "",
  }: Props = $props();

  const variantConfig = {
    running: {
      bg: "bg-[var(--success-soft)] text-[var(--success)] border-[var(--success)]/20",
      dot: "bg-[var(--success)]",
      defaultText: "运行中",
    },
    success: {
      bg: "bg-[var(--success-soft)] text-[var(--success)] border-[var(--success)]/20",
      dot: "bg-[var(--success)]",
      defaultText: "正常",
    },
    starting: {
      bg: "bg-[var(--warning-soft)] text-[var(--warning)] border-[var(--warning)]/20",
      dot: "bg-[var(--warning)]",
      defaultText: "启动中",
    },
    stopping: {
      bg: "bg-[var(--warning-soft)] text-[var(--warning)] border-[var(--warning)]/20",
      dot: "bg-[var(--warning)]",
      defaultText: "停止中",
    },
    warning: {
      bg: "bg-[var(--warning-soft)] text-[var(--warning)] border-[var(--warning)]/20",
      dot: "bg-[var(--warning)]",
      defaultText: "警告",
    },
    stopped: {
      bg: "bg-[var(--surface-hover)] text-[var(--text-muted)] border-[var(--border)]",
      dot: "bg-[var(--text-muted)]",
      defaultText: "已停止",
    },
    neutral: {
      bg: "bg-[var(--surface-hover)] text-[var(--text-secondary)] border-[var(--border)]",
      dot: "bg-[var(--text-muted)]",
      defaultText: "未配置",
    },
    error: {
      bg: "bg-[var(--danger-soft)] text-[var(--danger)] border-[var(--danger)]/20",
      dot: "bg-[var(--danger)]",
      defaultText: "异常",
    },
    info: {
      bg: "bg-[var(--primary-soft)] text-[var(--primary)] border-[var(--primary)]/20",
      dot: "bg-[var(--primary)]",
      defaultText: "提示",
    },
  };

  const current = $derived(variantConfig[status] ?? variantConfig.neutral);
  const displayText = $derived(text ?? current.defaultText);
</script>

<span
  class="inline-flex items-center gap-1.5 font-medium border rounded-full select-none whitespace-nowrap shrink-0 {size === 'sm' ? 'px-2 py-0.5 text-[10px]' : 'px-2.5 py-0.5 text-xs'} {current.bg} {customClass}"
>
  {#if showDot}
    <span class="size-1.5 rounded-full {current.dot}"></span>
  {/if}
  <span>{displayText}</span>
</span>
