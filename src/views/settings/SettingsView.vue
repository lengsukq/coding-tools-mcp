<script setup lang="ts">
import { computed, defineAsyncComponent, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import {
  Bot,
  Boxes,
  Globe2,
  Info,
  KeyRound,
  Laptop,
  MonitorUp,
  Network,
  Palette,
  PlugZap,
  Radar,
  Search,
  Server,
  ShieldCheck,
  Sparkles,
  TerminalSquare,
  Waypoints,
  X,
} from "@lucide/vue";
import PageShell from "$src/components/ui/PageShell.vue";
import GatewaySettingsView from "./GatewaySettingsView.vue";
import {
  SETTINGS_SIDEBAR_GROUPS,
  resolveActiveSettingsItemId,
  type SettingsNavItem,
} from "$lib/global-search";

const GeneralSettingsView = defineAsyncComponent(() => import("./GeneralSettingsView.vue"));
const UpstreamMcpSettingsView = defineAsyncComponent(() => import("./UpstreamMcpSettingsView.vue"));
const LanPeersSettingsView = defineAsyncComponent(() => import("./LanPeersSettingsView.vue"));
const ComputerSettingsView = defineAsyncComponent(() => import("./ComputerSettingsView.vue"));
const KeysSettingsView = defineAsyncComponent(() => import("./KeysSettingsView.vue"));
const FrpSettingsView = defineAsyncComponent(() => import("./FrpSettingsView.vue"));
const SoftwareSettingsView = defineAsyncComponent(() => import("./SoftwareSettingsView.vue"));

const route = useRoute();
const router = useRouter();
const filterQuery = ref("");

const section = computed(() => String(route.params.section ?? "general"));
const panel = computed(() => (route.query.panel ? String(route.query.panel) : undefined));
const activeItemId = computed(() => resolveActiveSettingsItemId(section.value, panel.value));

const activeViewGroup = computed(() => {
  switch (activeItemId.value) {
    case "gateway":
    case "gemini":
      return "gateway";
    case "upstream-mcp":
      return "upstream-mcp";
    case "lan":
    case "lan-peers":
      return "lan";
    case "computer-control":
    case "computer-audit":
      return "computer";
    case "keys":
      return "keys";
    case "frp":
      return "frp";
    case "software":
      return "software";
    default:
      return "general";
  }
});

const filteredGroups = computed(() => {
  const q = filterQuery.value.trim().toLowerCase();
  if (!q) return SETTINGS_SIDEBAR_GROUPS;
  return SETTINGS_SIDEBAR_GROUPS.map((group) => ({
    group: group.group,
    items: group.items.filter(
      (item) =>
        item.label.toLowerCase().includes(q) ||
        item.subtitle.toLowerCase().includes(q) ||
        item.keywords.some((kw) => kw.toLowerCase().includes(q)),
    ),
  })).filter((g) => g.items.length > 0);
});

const currentItem = computed<SettingsNavItem>(() => {
  for (const group of SETTINGS_SIDEBAR_GROUPS) {
    const found = group.items.find((item) => item.id === activeItemId.value);
    if (found) return found;
  }
  return SETTINGS_SIDEBAR_GROUPS[0].items[0];
});

const currentGroupName = computed(() => {
  for (const group of SETTINGS_SIDEBAR_GROUPS) {
    if (group.items.some((item) => item.id === activeItemId.value)) return group.group;
  }
  return "应用设置";
});

function selectItem(item: SettingsNavItem) {
  void router.push({
    path: `/settings/${item.section}`,
    ...(item.panel ? { query: { panel: item.panel } } : {}),
  });
}

function itemIcon(id: string) {
  switch (id) {
    case "gateway":
      return Globe2;
    case "upstream-mcp":
      return PlugZap;
    case "gemini":
      return Sparkles;
    case "auth":
      return ShieldCheck;
    case "lan":
      return Laptop;
    case "lan-peers":
      return Radar;
    case "keys":
      return KeyRound;
    case "frp":
      return Server;
    case "permissions":
      return TerminalSquare;
    case "context":
      return Bot;
    case "computer-control":
      return MonitorUp;
    case "computer-audit":
      return Waypoints;
    case "proxy":
      return Network;
    case "software":
      return Boxes;
    case "about":
      return Info;
    default:
      return Palette;
  }
}
</script>

<template>
  <section class="page-scroll">
    <PageShell>
      <div class="ui-subnav-layout">
        <!-- Left Secondary Matte Glass Navigation Rail -->
        <aside class="ui-subnav-rail" aria-label="设置分类导航">
          <div class="px-2 pb-1.5">
            <h1 class="text-[14px] font-semibold tracking-tight text-[var(--ui-text)]">应用设置</h1>
            <p class="mt-0.5 text-[11px] text-[var(--ui-text-muted)]">独立子页 · 保存即热更新</p>
          </div>

          <div class="relative mb-2 px-1">
            <Search :size="13" class="pointer-events-none absolute left-3.5 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)]" />
            <input
              v-model="filterQuery"
              type="text"
              aria-label="筛选设置页面"
              placeholder="筛选设置项…"
              class="h-7.5 w-full rounded-full border border-[var(--ui-line)] bg-[var(--ui-surface-subtle)] pl-8 pr-7 text-xs text-[var(--ui-text)] outline-none transition focus:border-[var(--ui-accent)] focus:bg-[var(--ui-surface-raised)]"
            />
            <button
              v-if="filterQuery"
              type="button"
              class="absolute right-3 top-1/2 -translate-y-1/2 text-[var(--ui-text-muted)] hover:text-[var(--ui-text)]"
              aria-label="清空筛选"
              @click="filterQuery = ''"
            >
              <X :size="12" />
            </button>
          </div>

          <div class="ui-subnav-scroll">
            <p v-if="!filteredGroups.length" class="px-2 py-6 text-center text-xs text-[var(--ui-text-muted)]">
              无匹配分类
            </p>
            <div v-for="group in filteredGroups" :key="group.group" class="ui-subnav-group">
              <div class="ui-subnav-group__title">{{ group.group }}</div>
              <button
                v-for="item in group.items"
                :key="item.id"
                type="button"
                class="ui-subnav-item"
                :class="{ active: item.id === activeItemId }"
                :aria-current="item.id === activeItemId ? 'page' : undefined"
                @click="selectItem(item)"
              >
                <component :is="itemIcon(item.id)" :size="14" class="shrink-0" />
                <span class="truncate">{{ item.label }}</span>
              </button>
            </div>
          </div>
        </aside>

        <!-- Right Single-Topic Content Stage -->
        <div class="ui-subnav-stage min-w-0 space-y-4">
          <header class="border-b border-[var(--ui-line)] pb-3.5">
            <div class="min-w-0">
              <span class="text-xs font-semibold text-[var(--ui-accent)]">{{ currentGroupName }}</span>
              <h2 class="mt-0.5 text-2xl font-semibold tracking-tight text-[var(--ui-text)]">{{ currentItem.label }}</h2>
              <p class="mt-1 text-[13px] text-[var(--ui-text-secondary)]">{{ currentItem.subtitle }}</p>
            </div>
          </header>

          <div :key="activeViewGroup" class="ui-stack animate-fade-in-up">
            <GatewaySettingsView v-if="activeViewGroup === 'gateway'" :panel="activeItemId === 'gemini' ? 'gemini' : 'gateway'" />
            <UpstreamMcpSettingsView v-else-if="activeViewGroup === 'upstream-mcp'" />
            <LanPeersSettingsView v-else-if="activeViewGroup === 'lan'" :panel="activeItemId === 'lan-peers' ? 'lan-peers' : 'lan'" />
            <ComputerSettingsView v-else-if="activeViewGroup === 'computer'" :panel="activeItemId === 'computer-audit' ? 'audit' : 'control'" />
            <KeysSettingsView v-else-if="activeViewGroup === 'keys'" />
            <FrpSettingsView v-else-if="activeViewGroup === 'frp'" />
            <SoftwareSettingsView v-else-if="activeViewGroup === 'software'" />
            <GeneralSettingsView
              v-else
              :section="currentItem.section"
              :panel="currentItem.panel ?? ''"
            />
          </div>
        </div>
      </div>
    </PageShell>
  </section>
</template>
