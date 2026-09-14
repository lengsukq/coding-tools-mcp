<script lang="ts">
  import type { ProxyConfigDto } from "$lib/api/settings";
  import Button from "$lib/components/ui/Button.svelte";
  import Select from "$lib/components/ui/Select.svelte";
  import TextInput from "$lib/components/ui/TextInput.svelte";

  interface Props {
    proxy: ProxyConfigDto;
    changed: boolean;
    saving: boolean;
    onChange: () => void;
    onSave: () => void | Promise<void>;
  }

  let { proxy, changed, saving, onChange, onSave }: Props = $props();
</script>

<div class="tx-card p-5">
  <h3 class="text-sm font-semibold tracking-tight text-[var(--text-main)]">网络代理</h3>
  <form class="mt-4 grid gap-3.5" onsubmit={(event) => { event.preventDefault(); void onSave(); }}>
    <div>
      <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1" for="proxy-mode-select">代理模式</label>
      <Select
        options={[
          { value: "none", label: "无代理" },
          { value: "system", label: "系统代理" },
          { value: "manual", label: "手动代理地址" },
        ]}
        bind:value={proxy.mode}
        onchange={onChange}
      />
    </div>
    {#if proxy.mode === "manual"}
      <div>
        <label class="block text-xs font-medium text-[var(--text-secondary)] mb-1" for="proxy-url-input">代理地址</label>
        <TextInput mono placeholder="http://127.0.0.1:7890" bind:value={proxy.url} oninput={onChange} />
        <span class="block text-[11px] text-[var(--color-text-muted)] mt-1">
          支持 HTTP/HTTPS/SOCKS 代理，如 http://127.0.0.1:7890
        </span>
      </div>
    {/if}
    <div class="flex justify-end pt-1">
      <Button type="submit" variant="primary" size="md" disabled={!changed || saving} busy={saving}>
        {saving ? "保存中…" : "保存设置"}
      </Button>
    </div>
  </form>
</div>
