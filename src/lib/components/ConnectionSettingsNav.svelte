<script lang="ts">
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import SegmentedControl from "$lib/components/ui/SegmentedControl.svelte";

  type ConnectionSection = "gateway" | "frp" | "software";

  const items = [
    { value: "gateway", label: "全局网关" },
    { value: "frp", label: "FRP 隧道" },
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

<div class="px-7 pt-4">
  <SegmentedControl
    {items}
    value={active}
    size="sm"
    onchange={(value) => goto(`/settings/${value as ConnectionSection}`)}
  />
</div>
