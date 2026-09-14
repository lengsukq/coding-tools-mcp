<script lang="ts">
  import type { Component } from "svelte";

  export interface SegmentItem {
    value: string;
    label: string;
    icon?: Component<{ size?: number; class?: string }>;
  }

  interface Props {
    items: SegmentItem[];
    value: string;
    size?: "sm" | "md";
    class?: string;
    onchange: (value: string) => void;
  }

  let {
    items,
    value,
    size = "md",
    class: customClass = "",
    onchange,
  }: Props = $props();
</script>

<div class="tx-segmented {customClass}" role="tablist">
  {#each items as item (item.value)}
    {@const Icon = item.icon}
    <button
      type="button"
      role="tab"
      aria-selected={value === item.value}
      class="tx-segmented-item {size === 'sm' ? '!px-2.5 !py-1 !text-xs' : '!px-3.5 !py-1.5 !text-xs'}"
      class:active={value === item.value}
      onclick={() => onchange(item.value)}
    >
      {#if Icon}
        <Icon size={size === "sm" ? 13 : 15} class="mr-1.5 shrink-0" />
      {/if}
      <span>{item.label}</span>
    </button>
  {/each}
</div>
