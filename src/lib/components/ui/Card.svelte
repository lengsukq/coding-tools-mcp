<script lang="ts">
  import type { Snippet } from "svelte";

  interface Props {
    class?: string;
    padding?: "none" | "sm" | "md" | "lg";
    hoverable?: boolean;
    onclick?: () => void;
    children?: Snippet;
  }

  let {
    class: customClass = "",
    padding = "md",
    hoverable = false,
    onclick,
    children,
  }: Props = $props();

  const paddingClasses = {
    none: "p-0",
    sm: "p-3",
    md: "p-4 sm:p-5",
    lg: "p-6",
  };
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div
  class="tx-card {paddingClasses[padding]} {hoverable ? 'cursor-pointer hover:border-[var(--primary)]/30 hover:shadow-md active:scale-[0.99] transition-all' : ''} {customClass}"
  {onclick}
  tabindex={onclick ? 0 : undefined}
  role={onclick ? "button" : undefined}
>
  {#if children}
    {@render children()}
  {/if}
</div>
