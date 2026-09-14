<script lang="ts">
  import type { Snippet } from "svelte";
  import { X } from "@lucide/svelte";

  interface Props {
    open: boolean;
    title?: string;
    description?: string;
    maxWidth?: string;
    showClose?: boolean;
    onclose?: () => void;
    children?: Snippet;
  }

  let {
    open,
    title,
    description,
    maxWidth = "max-w-md",
    showClose = true,
    onclose,
    children,
  }: Props = $props();

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && open) {
      event.preventDefault();
      onclose?.();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
  <div
    class="tx-modal-backdrop"
    role="presentation"
    onclick={(e) => {
      if (e.target === e.currentTarget) {
        onclose?.();
      }
    }}
  >
    <div
      class="tx-modal-card {maxWidth}"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
    >
      {#if title || showClose}
        <div class="flex items-start justify-between gap-4 mb-3">
          <div class="min-w-0">
            {#if title}
              <h3 class="text-base font-semibold text-[var(--text-main)] tracking-tight">{title}</h3>
            {/if}
            {#if description}
              <p class="text-xs text-[var(--text-muted)] mt-1 leading-relaxed">{description}</p>
            {/if}
          </div>
          {#if showClose && onclose}
            <button
              type="button"
              class="tx-icon-button -mr-1 -mt-1 p-1.5 rounded-full hover:bg-[var(--surface-hover)] text-[var(--text-muted)] hover:text-[var(--text-main)] transition-colors"
              aria-label="关闭"
              onclick={onclose}
            >
              <X size={16} />
            </button>
          {/if}
        </div>
      {/if}

      {#if children}
        {@render children()}
      {/if}
    </div>
  </div>
{/if}
