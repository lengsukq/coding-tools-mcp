<script lang="ts">
  import { ChevronDown } from "@lucide/svelte";

  export interface SelectOption {
    value: string;
    label: string;
  }

  interface Props {
    options: readonly SelectOption[] | SelectOption[];
    value?: string;
    disabled?: boolean;
    class?: string;
    onchange?: (value: string) => void;
  }

  let {
    options,
    value = $bindable(""),
    disabled = false,
    class: customClass = "",
    onchange,
  }: Props = $props();
</script>

<div class="relative inline-block w-full {customClass}">
  <select
    bind:value
    {disabled}
    class="w-full appearance-none px-3 py-1.5 pr-8 text-xs rounded-lg border border-[var(--border)] bg-[var(--card-bg)] text-[var(--text-main)] focus:outline-none focus:ring-2 focus:ring-[var(--primary)]/25 focus:border-[var(--primary)] transition-all cursor-pointer disabled:opacity-50 disabled:cursor-not-allowed"
    onchange={(e) => onchange?.(e.currentTarget.value)}
  >
    {#each options as opt (opt.value)}
      <option value={opt.value}>{opt.label}</option>
    {/each}
  </select>
  <div class="pointer-events-none absolute inset-y-0 right-0 flex items-center pr-2.5 text-[var(--text-muted)]">
    <ChevronDown size={14} strokeWidth={2} />
  </div>
</div>
