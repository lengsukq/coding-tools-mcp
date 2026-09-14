<script lang="ts">
  import { Activity, Boxes, Gauge, ListChecks, Network } from "@lucide/svelte";

  interface Props {
    activeSection: string;
    onNavigate: (event: MouseEvent, targetId: string) => void;
  }

  let { activeSection, onNavigate }: Props = $props();

  const items = [
    { id: "dashboard-overview", label: "运行总览", icon: Gauge },
    { id: "dashboard-metrics", label: "关键指标", icon: Activity },
    { id: "dashboard-workspaces", label: "运行矩阵", icon: Boxes },
    { id: "dashboard-usage", label: "Token 趋势", icon: Network },
    { id: "dashboard-details", label: "连接与 Planning", icon: ListChecks },
  ];
</script>

<nav class="tx-dashboard-quick-nav" aria-label="Dashboard 快速导航">
  <span class="tx-dashboard-quick-nav-label">快速跳转</span>
  {#each items as item}
    <a
      href={`#${item.id}`}
      class:active={activeSection === item.id}
      aria-label={`跳转到${item.label}`}
      title={item.label}
      onclick={(event) => onNavigate(event, item.id)}
    >
      <item.icon size={15} />
    </a>
  {/each}
</nav>
