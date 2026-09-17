<script setup lang="ts">
import { useRoute, useRouter } from "vue-router";
import SegmentedControl from "../ui/SegmentedControl.vue";
const route = useRoute();
const router = useRouter();
const items = [{ value: "/settings/gateway", label: "Global Gateway" }, { value: "/settings/frp", label: "FRP" }, { value: "/settings/software", label: "软件" }];
</script>

<template>
  <nav class="connection-subnav mb-5" aria-label="连接设置分类">
    <SegmentedControl aria-label="连接设置分类" :items="items" :model-value="route.path" @update:model-value="router.push($event)" />
  </nav>
</template>

<style scoped>
.connection-subnav :deep(.ios-segmented) {
  padding: 2px;
  border-color: color-mix(in srgb, var(--text-main) 5%, transparent);
  background: color-mix(in srgb, var(--text-main) 5%, transparent);
  box-shadow: none;
}

.connection-subnav :deep(.ios-segmented__item) {
  min-height: 27px;
  padding: 4px 10px;
  font-size: 10.5px;
}
</style>
