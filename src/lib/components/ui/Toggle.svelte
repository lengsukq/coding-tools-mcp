<script lang="ts">
  interface Props {
    checked?: boolean;
    label?: string;
    description?: string;
    disabled?: boolean;
    class?: string;
    onchange?: (checked: boolean) => void;
  }

  let {
    checked = $bindable(false),
    label,
    description,
    disabled = false,
    class: customClass = "",
    onchange,
  }: Props = $props();

  function toggle() {
    if (disabled) return;
    checked = !checked;
    onchange?.(checked);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      toggle();
    }
  }
</script>

<div
  class="flex items-center justify-between gap-4 cursor-pointer select-none {disabled ? 'opacity-50 cursor-not-allowed' : ''} {customClass}"
  onclick={toggle}
  onkeydown={handleKeydown}
  role="switch"
  aria-checked={checked}
  tabindex={disabled ? -1 : 0}
>
  {#if label || description}
    <div class="min-w-0 flex-1">
      {#if label}
        <div class="text-sm font-medium text-[var(--text-main)] leading-snug">{label}</div>
      {/if}
      {#if description}
        <div class="text-xs text-[var(--text-muted)] mt-0.5 leading-relaxed">{description}</div>
      {/if}
    </div>
  {/if}

  <div class="tx-switch-track" class:active={checked} class:opacity-50={disabled}>
    <div class="tx-switch-thumb"></div>
  </div>
</div>
