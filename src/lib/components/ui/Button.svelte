<script lang="ts">
  import type { Snippet } from "svelte";
  import { Loader2 } from "@lucide/svelte";

  interface Props {
    type?: "button" | "submit" | "reset";
    variant?: "primary" | "secondary" | "ghost" | "danger" | "icon";
    size?: "sm" | "md" | "lg";
    disabled?: boolean;
    busy?: boolean;
    class?: string;
    title?: string;
    ariaLabel?: string;
    onclick?: (event: MouseEvent) => void | Promise<void>;
    children?: Snippet;
  }

  let {
    type = "button",
    variant = "primary",
    size = "md",
    disabled = false,
    busy = false,
    class: customClass = "",
    title,
    ariaLabel,
    onclick,
    children,
  }: Props = $props();

  const sizeClasses = {
    sm: "px-2.5 py-1 text-xs gap-1.5 rounded-[8px]",
    md: "px-3.5 py-1.5 text-xs font-medium gap-2 rounded-[10px]",
    lg: "px-4 py-2 text-sm font-semibold gap-2.5 rounded-[12px]",
  };

  const iconSizeClasses = {
    sm: "p-1.5 rounded-[8px]",
    md: "p-2 rounded-[10px]",
    lg: "p-2.5 rounded-[12px]",
  };

  const variantClasses = {
    primary: "tx-btn-primary",
    danger: "tx-btn-primary tx-btn-danger",
    ghost: "tx-btn-ghost",
    secondary: "bg-[var(--surface-hover)] border border-[var(--border)] text-[var(--text-main)] hover:bg-[var(--border)] active:scale-[0.975] transition-all",
    icon: "tx-icon-button hover:bg-[var(--surface-hover)] text-[var(--text-secondary)] hover:text-[var(--text-main)] transition-all",
  };
</script>

<button
  {type}
  disabled={disabled || busy}
  class="{variant === 'icon' ? iconSizeClasses[size] : sizeClasses[size]} {variantClasses[variant]} inline-flex items-center justify-center font-medium cursor-pointer transition-all disabled:opacity-50 disabled:cursor-not-allowed select-none {customClass}"
  {title}
  aria-label={ariaLabel}
  onclick={(e) => {
    if (!disabled && !busy && onclick) {
      void onclick(e);
    }
  }}
>
  {#if busy}
    <Loader2 size={size === "sm" ? 12 : size === "lg" ? 18 : 15} class="animate-spin shrink-0" />
  {/if}
  {#if children}
    {@render children()}
  {/if}
</button>
