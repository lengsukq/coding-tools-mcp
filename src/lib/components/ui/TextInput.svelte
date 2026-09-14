<script lang="ts">
  interface Props {
    value?: string;
    type?: string;
    placeholder?: string;
    disabled?: boolean;
    readonly?: boolean;
    mono?: boolean;
    class?: string;
    onchange?: (value: string) => void;
    oninput?: (value: string) => void;
    onkeydown?: (event: KeyboardEvent) => void;
  }

  let {
    value = $bindable(""),
    type = "text",
    placeholder = "",
    disabled = false,
    readonly = false,
    mono = false,
    class: customClass = "",
    onchange,
    oninput,
    onkeydown,
  }: Props = $props();
</script>

<input
  {type}
  {placeholder}
  {disabled}
  {readonly}
  bind:value
  class="w-full px-3 py-1.5 text-xs rounded-lg border border-[var(--border)] bg-[var(--card-bg)] text-[var(--text-main)] placeholder:text-[var(--text-muted)] focus:outline-none focus:ring-2 focus:ring-[var(--primary)]/25 focus:border-[var(--primary)] transition-all disabled:opacity-50 disabled:cursor-not-allowed {mono ? 'font-mono' : ''} {customClass}"
  onchange={(e) => onchange?.(e.currentTarget.value)}
  oninput={(e) => oninput?.(e.currentTarget.value)}
  {onkeydown}
/>
