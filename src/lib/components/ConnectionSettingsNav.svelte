<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import Tabs from "$lib/components/Tabs.svelte";

  type ConnectionSection = "gateway" | "frp" | "software";

  const items = [
    { value: "gateway", label: "全局网关" },
    { value: "frp", label: "FRP" },
    { value: "software", label: "隧道工具" },
  ];

  const active = $derived<ConnectionSection>(
    $page.url.pathname.startsWith("/settings/frp")
      ? "frp"
      : $page.url.pathname.startsWith("/settings/software")
        ? "software"
        : "gateway",
  );
</script>

<div class="px-6 pt-4">
  <Tabs
    {items}
    value={active}
    onchange={(value) => goto(`/settings/${value as ConnectionSection}`)}
  />
</div>
